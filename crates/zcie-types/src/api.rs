//! API request and response types (`docs/10-api-and-gateways.md` sections 2 and 3.1).

use serde::{Deserialize, Serialize};

use crate::edge::Relation;
use crate::event::{Classification, EventSource, GitContext};
use crate::query::Watermarks;

/// Capture request: client fields of the envelope plus the payload.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct CaptureRequest {
    /// Must be `zcie.event.v1`.
    pub schema: String,
    /// Client idempotency key. 1 to 256 characters.
    pub idempotency_key: String,
    /// RFC 3339 time. The daemon sets it if it is missing.
    #[serde(default)]
    pub timestamp_utc: Option<String>,
    /// Workspace name or `ws_` ID.
    pub workspace: String,
    /// Optional repository ID.
    #[serde(default)]
    pub repository_id: Option<String>,
    /// Client source.
    pub source: EventSource,
    /// Event type name, for example `user.message`.
    #[serde(rename = "type")]
    pub event_type: String,
    /// Payload text. JSON payloads are sent as a string.
    #[serde(default)]
    pub payload: Option<String>,
    /// Git state of the client.
    #[serde(default)]
    pub git: Option<GitContext>,
    /// Privacy class. Default `project_private`.
    #[serde(default)]
    pub classification: Option<Classification>,
}

/// Capture receipt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CaptureReceipt {
    /// The event ID.
    pub event_id: String,
    /// The event sequence.
    pub seq: u64,
    /// True after the sync write.
    pub durable: bool,
    /// Always false at return time.
    pub indexed: bool,
    /// True if the idempotency key existed.
    pub duplicate: bool,
    /// Canonical watermark after the capture.
    pub canonical_seq: u64,
}

/// Repository registration request.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct RegisterRepositoryRequest {
    /// Workspace name or `ws_` ID.
    pub workspace: String,
    /// Path to the repository folder.
    pub path: String,
    /// Optional project name.
    #[serde(default)]
    pub project: Option<String>,
}

/// Repository registration response.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegisterRepositoryResponse {
    /// Repository ID.
    pub repository_id: String,
    /// Worktree ID.
    pub worktree_id: String,
    /// The scan job ID.
    pub scan_job_id: String,
    /// True if the folder is a Git repository.
    pub git_available: bool,
}

/// Task creation request.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct CreateTaskRequest {
    /// Workspace name or `ws_` ID.
    pub workspace: String,
    /// Task title.
    pub title: String,
    /// External ref, for example `#89`.
    #[serde(default)]
    pub external_ref: Option<String>,
    /// The event that requested the task.
    pub source_event_id: String,
}

/// Link request. Evidence is `user_confirmation` or `client_trace` only.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LinkRequest {
    /// Workspace name or `ws_` ID.
    pub workspace: String,
    /// Source ref.
    pub from: String,
    /// Relation.
    pub relation: Relation,
    /// Target ref.
    pub to: String,
    /// `user_confirmation` or `client_trace`.
    pub evidence: String,
    /// The event that supports the link.
    pub source_event_id: String,
    /// Short note.
    #[serde(default)]
    pub note: Option<String>,
}

/// Link response.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LinkResponse {
    /// The edge ID.
    pub edge_id: String,
}

/// Memory correction request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CorrectRequest {
    /// Corrected text.
    pub text: String,
    /// The event that contains the correction.
    pub source_event_id: String,
}

/// Forget request.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ForgetRequest {
    /// Workspace name or `ws_` ID.
    pub workspace: String,
    /// What to forget.
    pub selector: ForgetSelector,
    /// Reason for the audit record.
    #[serde(default)]
    pub reason: String,
}

/// Forget selector. Set one field.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ForgetSelector {
    /// One entity or event ref.
    #[serde(default, rename = "ref")]
    pub entity_ref: Option<String>,
    /// All events of a session.
    #[serde(default)]
    pub session_id: Option<String>,
    /// All events before this RFC 3339 time.
    #[serde(default)]
    pub before_utc: Option<String>,
}

/// Forget receipt.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ForgetReceipt {
    /// Number of events removed.
    pub events_removed: u64,
    /// Number of memory records retracted.
    pub records_retracted: u64,
    /// Number of edges removed.
    pub edges_removed: u64,
    /// Number of search delete operations sent.
    pub pending_jobs: u64,
    /// RFC 3339 time of the operation.
    pub utc: String,
}

/// One timeline item.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimelineItem {
    /// Event sequence.
    pub seq: u64,
    /// Event ID.
    pub event_id: String,
    /// Event type name.
    pub event_type: String,
    /// RFC 3339 event time.
    pub timestamp_utc: String,
    /// First 160 characters of the payload.
    pub summary: String,
    /// Entities linked to the event.
    pub refs: Vec<String>,
}

/// Timeline response.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimelineResponse {
    /// Items in sequence order.
    pub items: Vec<TimelineItem>,
}

/// Health report.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HealthReport {
    /// `ok`.
    pub status: String,
    /// Daemon version.
    pub version: String,
    /// Current watermarks. Only with a key.
    #[serde(default)]
    pub watermarks: Option<Watermarks>,
    /// Queue sizes. Only with a key.
    #[serde(default)]
    pub queue: Option<QueueReport>,
    /// Size of the instance folder in bytes. Only with a key.
    #[serde(default)]
    pub storage_bytes: Option<u64>,
    /// Daemon uptime in seconds.
    pub uptime_s: u64,
}

/// Queue sizes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct QueueReport {
    /// Pending interactive jobs in memory.
    pub interactive: u64,
    /// Pending background jobs in memory.
    pub background: u64,
    /// Jobs in the dead-letter list.
    pub dead_letter: u64,
}

/// Rebuild response.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobResponse {
    /// The job ID.
    pub job_id: String,
}

/// HTTP error body: `{ "error": { "code", "message" } }`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ErrorBody {
    /// The error.
    pub error: ErrorDetail,
}

/// HTTP error detail.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ErrorDetail {
    /// Machine-readable code, for example `invalid_request`.
    pub code: String,
    /// Human-readable message.
    pub message: String,
}

/// Stored value of an idempotency key: the receipt fields.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CaptureReceiptValue {
    /// Value schema version.
    pub v: u32,
    /// The event ID.
    pub event_id: String,
    /// The event sequence.
    pub seq: u64,
}
