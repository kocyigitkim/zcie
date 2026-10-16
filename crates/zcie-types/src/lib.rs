//! Shared types of ZCIE: IDs, scope, events, records, edges, jobs, query and API types.
//!
//! This crate has no I/O. See `docs/05-data-model.md`.

pub mod api;
pub mod code;
pub mod edge;
pub mod error;
pub mod event;
pub mod ids;
pub mod job;
pub mod memory;
pub mod query;
pub mod registry;
pub mod scope;
pub mod task;

pub use code::{CommitRecord, FileRecord, ParserQuality, SymbolKind, SymbolRecord};
pub use edge::{Edge, EdgeStatus, Evidence, EvidenceKind, Relation};
pub use error::{Error, Result};
pub use event::{
    Classification, CommandRunPayload, ConfirmationPayload, EventEnvelope, EventSource, EventType, FileChangePayload,
    GitCommitPayload, GitContext, TestResultPayload,
};
pub use ids::{CommitId, EdgeId, EventId, FileId, JobId, MemoryId, Ref, SymbolId, TaskId};
pub use job::{JobKind, JobPriority, JobRecord, JobState};
pub use memory::{MemoryKind, MemoryRecord, MemoryStatus};
pub use query::{
    AnswerKind, ExpandItem, ExpandLevel, ExpandRequest, ExpandResponse, ExpandStatus, Freshness, QueryScope, QueryClass, QueryMode, QueryRequest, QueryResponse, ResultItem, Watermarks};
pub use registry::{ProjectRecord, RepositoryRecord, WorkspaceRecord, WorktreeRecord};
pub use api::CaptureReceiptValue;
pub use scope::{LOCAL_TENANT, Scope, now_utc};
pub use task::{TaskRecord, TaskState, TaskStateChange};

/// Estimates model tokens of a text: `ceil(chars / 4)`.
pub fn estimate_tokens(text: &str) -> u32 {
    let chars = text.chars().count() as u32;
    chars.div_ceil(4)
}

/// Returns the first `max` characters of a text.
pub fn truncate_chars(text: &str, max: usize) -> String {
    text.chars().take(max).collect()
}
