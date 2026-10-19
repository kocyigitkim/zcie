//! Small lookup values (`docs/06-storage.md` section 3.6).

use serde::{Deserialize, Serialize};

/// Empty marker value `{"v":1}`.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct Marker {
    /// Value schema version.
    pub v: u32,
}

impl Marker {
    /// Makes a version 1 marker.
    pub fn new() -> Self {
        Self { v: 1 }
    }
}

/// Lookup value `t/"path"/<worktree>/<path>` → file ID.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileIdValue {
    /// Value schema version.
    pub v: u32,
    /// The file ID.
    pub file_id: String,
}

/// Lookup value `t/"file_refs"/<file id>`: the names that a file uses and does not define, with
/// their counts (ADR-017). The repository map joins them with the symbol definitions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileRefsValue {
    /// Value schema version.
    pub v: u32,
    /// (name, count), the most used first.
    pub names: Vec<(String, u32)>,
}

/// Lookup value `t/"file_calls"/<file id>` → the imports and the call sites of a file (ADR-019,
/// D-071). The query resolves the calls to definitions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileCallsValue {
    /// Value schema version.
    pub v: u32,
    /// Raw import strings of the file.
    pub imports: Vec<String>,
    /// Call sites in source order.
    pub calls: Vec<zcie_types::CallSite>,
}

/// Lookup value `t/"ws_name"/<name>` → workspace ID.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceIdValue {
    /// Value schema version.
    pub v: u32,
    /// The workspace ID.
    pub workspace_id: String,
}

/// Lookup value `t/"task_ext"/<workspace>/<ref>` → task ID.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskIdValue {
    /// Value schema version.
    pub v: u32,
    /// The task ID.
    pub task_id: String,
}

/// Lookup value `t/"commit_sha"/<repository>/<sha>` → commit ID.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitIdValue {
    /// Value schema version.
    pub v: u32,
    /// The commit ID.
    pub commit_id: String,
}

/// Lookup value `t/"turn"/<session>/<turn>/<event id>` → event type and seq.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TurnValue {
    /// Value schema version.
    pub v: u32,
    /// Event type name.
    pub event_type: String,
    /// Event sequence.
    pub seq: u64,
}

/// Lookup value `t/"code_seq"/<workspace>` → a counter of the file changes of a workspace. The
/// repository map and the cached query answers change with it (ADR-017, D-078).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeSeqValue {
    /// Value schema version.
    pub v: u32,
    /// Number of file changes so far.
    pub seq: u64,
}

/// Tombstone value in CF `policies`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TombstoneValue {
    /// Value schema version.
    pub v: u32,
    /// RFC 3339 time of the forget operation.
    pub forgotten_utc: String,
    /// Reason of the operation.
    pub reason: String,
}

/// Value `revisions: t/"file_state"/<worktree>/<path>` → current content hash.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileStateValue {
    /// Value schema version.
    pub v: u32,
    /// Content hash of the indexed version.
    pub content_hash: String,
    /// Sequence of the indexed version.
    pub seq: u64,
}
