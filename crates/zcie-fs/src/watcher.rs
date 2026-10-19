//! Debounced file watcher (`docs/07-ingest-and-indexing.md` section 5).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, channel};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use ignore::gitignore::{Gitignore, GitignoreBuilder};
use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use zcie_security::ExcludeSet;

use crate::discover::relative_path;
use crate::error::{Error, Result};

/// Poll interval of the debounce thread.
const TICK: Duration = Duration::from_millis(50);

/// The kind of a file change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeKind {
    /// The file was created or changed.
    Modified,
    /// The file was removed.
    Deleted,
}

/// One debounced file change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileChange {
    /// Path relative to the watched root.
    pub rel_path: String,
    /// The change kind.
    pub kind: ChangeKind,
}

/// Callback for each debounced change.
pub type ChangeHandler = Box<dyn Fn(FileChange) + Send + 'static>;

/// Watches one folder recursively. Dropping the watcher stops it.
pub struct FileWatcher {
    _watcher: RecommendedWatcher,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl FileWatcher {
    /// Starts a watcher on `root`. Calls `on_change` once per path after `debounce`.
    pub fn start(
        root: PathBuf,
        debounce: Duration,
        exclude: ExcludeSet,
        on_change: ChangeHandler,
    ) -> Result<FileWatcher> {
        let (sender, receiver) = channel();
        let mut watcher =
            notify::recommended_watcher(sender).map_err(|e| Error::Notify(e.to_string()))?;
        watcher
            .watch(&root, RecursiveMode::Recursive)
            .map_err(|e| Error::Notify(e.to_string()))?;
        let stop = Arc::new(AtomicBool::new(false));
        let gitignore = root_gitignore(&root);
        let state = DebounceState { root, debounce, exclude, gitignore, on_change, stop: Arc::clone(&stop) };
        let thread = std::thread::Builder::new()
            .name("zcie-watcher".to_string())
            .spawn(move || state.run(receiver))?;
        Ok(FileWatcher { _watcher: watcher, stop, thread: Some(thread) })
    }
}

impl Drop for FileWatcher {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// The `.gitignore` and `.git/info/exclude` rules of the root, the same rules as a scan uses for
/// the top folder: changes of build outputs (`build/`, `target/`, `.build/`) make no jobs.
fn root_gitignore(root: &Path) -> Gitignore {
    let mut builder = GitignoreBuilder::new(root);
    for file in [root.join(".gitignore"), root.join(".git/info/exclude")] {
        if file.is_file() {
            let _ = builder.add(file);
        }
    }
    builder.build().unwrap_or_else(|_| Gitignore::empty())
}

struct DebounceState {
    root: PathBuf,
    debounce: Duration,
    exclude: ExcludeSet,
    gitignore: Gitignore,
    on_change: ChangeHandler,
    stop: Arc<AtomicBool>,
}

impl DebounceState {
    fn run(self, receiver: Receiver<notify::Result<notify::Event>>) {
        let mut pending: HashMap<String, Instant> = HashMap::new();
        while !self.stop.load(Ordering::Relaxed) {
            match receiver.recv_timeout(TICK) {
                Ok(Ok(event)) => self.collect(&event, &mut pending),
                Ok(Err(error)) => tracing::warn!(%error, "file watcher error"),
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => break,
            }
            self.flush(&mut pending);
        }
    }

    fn collect(&self, event: &notify::Event, pending: &mut HashMap<String, Instant>) {
        if event.kind.is_access() {
            return;
        }
        for path in &event.paths {
            let Some(rel_path) = self.relative(path) else { continue };
            if self.exclude.is_excluded(&rel_path) || self.gitignore.matched_path_or_any_parents(&rel_path, path.is_dir()).is_ignore() {
                continue;
            }
            pending.insert(rel_path, Instant::now());
        }
    }

    fn relative(&self, path: &Path) -> Option<String> {
        relative_path(&self.root, path).or_else(|| {
            let canonical = self.root.canonicalize().ok()?;
            relative_path(&canonical, path)
        })
    }

    fn flush(&self, pending: &mut HashMap<String, Instant>) {
        let ready: Vec<String> = pending
            .iter()
            .filter(|(_, at)| at.elapsed() >= self.debounce)
            .map(|(path, _)| path.clone())
            .collect();
        for rel_path in ready {
            pending.remove(&rel_path);
            let abs = self.root.join(&rel_path);
            let kind = if abs.exists() { ChangeKind::Modified } else { ChangeKind::Deleted };
            if kind == ChangeKind::Modified && abs.is_dir() {
                continue;
            }
            (self.on_change)(FileChange { rel_path, kind });
        }
    }
}
