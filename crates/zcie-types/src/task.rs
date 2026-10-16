//! Task entity (`docs/05-data-model.md` section 6).

use serde::{Deserialize, Serialize};

use crate::ids::{EventId, TaskId};
use crate::scope::Scope;

/// One work item.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TaskRecord {
    /// Value schema version.
    pub v: u32,
    /// The task ID.
    pub task_id: TaskId,
    /// The task scope.
    pub scope: Scope,
    /// Short title.
    pub title: String,
    /// Current state.
    pub state: TaskState,
    /// External ref, for example `#89`.
    #[serde(default)]
    pub external_ref: Option<String>,
    /// The event that made the task.
    pub created_from: EventId,
    /// RFC 3339 creation time.
    #[serde(default)]
    pub created_utc: String,
    /// Earlier states, oldest first.
    #[serde(default)]
    pub history: Vec<TaskStateChange>,
    /// Increments on each change.
    pub generation: u64,
}

/// The state of a task.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskState {
    /// Not started.
    Open,
    /// Work started.
    InProgress,
    /// A test passed for the task.
    Resolved,
    /// A test failed for the task.
    Failed,
    /// The task was stopped.
    Abandoned,
}

impl TaskState {
    /// Returns the serialized name.
    pub fn name(&self) -> &'static str {
        match self {
            TaskState::Open => "open",
            TaskState::InProgress => "in_progress",
            TaskState::Resolved => "resolved",
            TaskState::Failed => "failed",
            TaskState::Abandoned => "abandoned",
        }
    }
}

/// One earlier state of a task.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskStateChange {
    /// The state before the change.
    pub from: TaskState,
    /// The state after the change.
    pub to: TaskState,
    /// The event that caused the change.
    pub event_id: EventId,
    /// RFC 3339 time of the change.
    pub utc: String,
}
