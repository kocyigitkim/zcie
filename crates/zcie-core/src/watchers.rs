//! File watchers per worktree (`docs/07-ingest-and-indexing.md` section 5).

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use zcie_api::Engine;
use zcie_fs::{ChangeKind, FileWatcher};
use zcie_ingest::Ingest;
use zcie_store::{Cf, layout};
use zcie_types::{JobKind, JobPriority, JobRecord, LOCAL_TENANT, Scope, WorkspaceRecord, WorktreeRecord};

use crate::error::Result;

/// Maximum registered worktrees.
const MAX_WORKTREES: usize = 10_000;

/// Running file watchers, one per worktree.
pub struct Watchers {
    ingest: Arc<Ingest>,
    debounce: Duration,
    running: Mutex<HashMap<String, FileWatcher>>,
}

impl Watchers {
    /// Makes an empty watcher list.
    pub fn new(ingest: Arc<Ingest>, debounce: Duration) -> Watchers {
        Watchers { ingest, debounce, running: Mutex::new(HashMap::new()) }
    }

    /// Makes the engine start a watcher for each new worktree.
    pub fn attach(self: &Arc<Self>, engine: &Engine) {
        let me = Arc::clone(self);
        engine.set_worktree_hook(Box::new(move |worktree| me.start(worktree)));
        let me = Arc::clone(self);
        engine.set_worktree_removed_hook(Box::new(move |worktree| me.stop(&worktree.worktree_id)));
    }

    /// Stops the watcher of one worktree.
    pub fn stop(&self, worktree_id: &str) {
        self.running.lock().unwrap_or_else(|e| e.into_inner()).remove(worktree_id);
    }

    /// Starts watchers for the indexed worktrees and makes one reconciliation scan for each
    /// (recovery step 11). A folder that only maps the memory to a project is not watched (ADR-015).
    pub fn start_all_and_reconcile(&self, engine: &Engine) -> Result<()> {
        let prefix = layout::registry_prefix(LOCAL_TENANT, "worktree");
        for (_, worktree) in engine.store.prefix_scan::<WorktreeRecord>(Cf::Registry, &prefix, MAX_WORKTREES)? {
            if !worktree.indexed {
                continue;
            }
            self.start(&worktree);
            let scope = self.scope_of(&worktree);
            let job = JobRecord::new(JobKind::ScanRepository, scope, JobPriority::Background, self.ingest.event_seq(), worktree.worktree_id.clone());
            self.ingest.enqueue_job(job)?;
        }
        Ok(())
    }

    fn scope_of(&self, worktree: &WorktreeRecord) -> Scope {
        let key = layout::registry(LOCAL_TENANT, "workspace", &worktree.workspace_id);
        let principal = self
            .ingest
            .store
            .get::<WorkspaceRecord>(Cf::Registry, &key)
            .ok()
            .flatten()
            .map(|w| w.principal_id)
            .unwrap_or_default();
        Scope {
            repository_id: Some(worktree.repository_id.clone()),
            worktree_id: Some(worktree.worktree_id.clone()),
            ..Scope::workspace(&principal, &worktree.workspace_id)
        }
    }

    /// Starts the watcher of one worktree. A running watcher is not started again.
    pub fn start(&self, worktree: &WorktreeRecord) {
        if !worktree.indexed {
            return;
        }
        // The lock is not held while the watcher starts: on macOS the first access to a protected
        // folder can wait for a privacy prompt, and a project delete must not wait for it.
        if self.running.lock().unwrap_or_else(|e| e.into_inner()).contains_key(&worktree.worktree_id) {
            return;
        }
        let ingest = Arc::clone(&self.ingest);
        let scope = self.scope_of(worktree);
        let worktree_id = worktree.worktree_id.clone();
        let handler = Box::new(move |change: zcie_fs::FileChange| {
            let kind = match change.kind {
                ChangeKind::Modified => JobKind::IndexFile,
                ChangeKind::Deleted => JobKind::DeleteFile,
            };
            if let Err(error) = ingest.enqueue_file_job(&scope, &worktree_id, &change.rel_path, kind) {
                tracing::warn!(%error, "file job could not be stored");
            }
        });
        let root = PathBuf::from(&worktree.root_path);
        match FileWatcher::start(root, self.debounce, self.ingest.config.exclude.clone(), handler) {
            // A project deleted while its watcher started gets no watcher.
            Ok(watcher) if self.ingest.store.exists(Cf::Registry, &layout::registry(LOCAL_TENANT, "worktree", &worktree.worktree_id)).unwrap_or(false) => {
                self.running.lock().unwrap_or_else(|e| e.into_inner()).entry(worktree.worktree_id.clone()).or_insert(watcher);
            }
            Ok(_) => {}
            Err(error) => tracing::warn!(%error, root = %worktree.root_path, "file watcher did not start"),
        }
    }

    /// Stops all watchers.
    pub fn stop_all(&self) {
        self.running.lock().unwrap_or_else(|e| e.into_inner()).clear();
    }
}
