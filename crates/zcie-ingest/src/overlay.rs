//! Live overlay: paths with a pending file job (`docs/07-ingest-and-indexing.md` section 10).

use std::collections::HashSet;
use std::sync::Mutex;

/// The set of `(worktree_id, path)` with a pending `IndexFile` or `DeleteFile` job.
#[derive(Debug, Default)]
pub struct LiveOverlay {
    paths: Mutex<HashSet<(String, String)>>,
}

impl LiveOverlay {
    fn lock(&self) -> std::sync::MutexGuard<'_, HashSet<(String, String)>> {
        self.paths.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Adds a path.
    pub fn add(&self, worktree_id: &str, path: &str) {
        self.lock().insert((worktree_id.to_string(), path.to_string()));
    }

    /// Removes a path.
    pub fn remove(&self, worktree_id: &str, path: &str) {
        self.lock().remove(&(worktree_id.to_string(), path.to_string()));
    }

    /// Returns true if any worktree has a pending job for the path.
    pub fn contains_path(&self, path: &str) -> bool {
        self.lock().iter().any(|(_, p)| p == path)
    }

    /// Returns the number of paths.
    pub fn len(&self) -> usize {
        self.lock().len()
    }

    /// Returns true if no path is pending.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}
