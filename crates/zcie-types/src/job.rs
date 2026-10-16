//! Durable background jobs (`docs/07-ingest-and-indexing.md` section 3.1).

use serde::{Deserialize, Serialize};

use crate::ids::JobId;
use crate::scope::{Scope, now_utc};

/// One durable unit of background work.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JobRecord {
    /// Value schema version.
    pub v: u32,
    /// The job ID.
    pub job_id: JobId,
    /// The job kind.
    pub kind: JobKind,
    /// The job scope.
    pub scope: Scope,
    /// Queue priority.
    pub priority: JobPriority,
    /// Event sequence that made the job.
    pub source_seq: u64,
    /// Event ID, `<worktree_id>:<path>`, commit SHA or repository ID.
    pub target: String,
    /// Number of failed attempts.
    pub attempts: u32,
    /// Current state.
    pub state: JobState,
    /// Last error text.
    #[serde(default)]
    pub last_error: Option<String>,
    /// RFC 3339 creation time.
    pub created_utc: String,
    /// RFC 3339 time of the last change.
    pub updated_utc: String,
}

impl JobRecord {
    /// Makes a new pending job.
    pub fn new(kind: JobKind, scope: Scope, priority: JobPriority, source_seq: u64, target: String) -> Self {
        let now = now_utc();
        Self {
            v: 1,
            job_id: JobId::new(),
            kind,
            scope,
            priority,
            source_seq,
            target,
            attempts: 0,
            state: JobState::Pending,
            last_error: None,
            created_utc: now.clone(),
            updated_utc: now,
        }
    }
}

/// The kind of a job.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum JobKind {
    /// Tier A extraction of one event.
    ExtractMemory,
    /// Hash, parse and store one file.
    IndexFile,
    /// Mark one file deleted.
    DeleteFile,
    /// Full scan of one worktree.
    ScanRepository,
    /// Store one commit and its links.
    IndexCommit,
    /// Rebuild the search index.
    UpdateSearch,
}

/// Queue priority of a job.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum JobPriority {
    /// Before background work.
    Interactive,
    /// After interactive work.
    Background,
}

/// The state of a job.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum JobState {
    /// Waits in the queue.
    Pending,
    /// A worker runs it.
    Running,
    /// Complete.
    Done,
    /// Failed three times.
    DeadLetter,
}

/// Makes the target of a file job: `<worktree_id>:<path>`.
pub fn file_target(worktree_id: &str, path: &str) -> String {
    format!("{worktree_id}:{path}")
}

/// Splits a file job target into the worktree ID and the path.
pub fn split_file_target(target: &str) -> Option<(&str, &str)> {
    target.split_once(':')
}
