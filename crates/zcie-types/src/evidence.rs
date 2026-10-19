//! Action-specific evidence (ADR-019, D-068 and D-069): the intent of a prompt and the exact
//! source slices that the next action of the agent needs.

use serde::{Deserialize, Serialize};

/// The next action that a prompt asks for. The planner selects it with no model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QueryIntent {
    /// Explain how code works ("how does", "explain", "açıkla").
    Explain,
    /// Find the files to change ("which file must I change").
    Locate,
    /// Change code ("add", "rename", "ekle", "değiştir").
    Edit,
    /// Follow a flow through the code ("trace", "from X to Y").
    Trace,
    /// Recall decisions, rules or history.
    Memory,
    /// Any other prompt.
    General,
}

impl QueryIntent {
    /// Returns the snake_case name.
    pub fn name(self) -> &'static str {
        match self {
            QueryIntent::Explain => "explain",
            QueryIntent::Locate => "locate",
            QueryIntent::Edit => "edit",
            QueryIntent::Trace => "trace",
            QueryIntent::Memory => "memory",
            QueryIntent::General => "general",
        }
    }
}

/// True when no intent or the intent `general` is set (the serde skip rule of `QueryResponse::intent`).
pub fn is_general(intent: &Option<QueryIntent>) -> bool {
    matches!(intent, None | Some(QueryIntent::General))
}

/// One exact source slice. The text, the line range and the hash come from one file revision.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EvidenceItem {
    /// The symbol ref.
    #[serde(rename = "ref")]
    pub entity_ref: String,
    /// Project ID.
    pub project: String,
    /// File path, relative to the root folder.
    pub path: String,
    /// First line (1-based).
    pub start_line: u32,
    /// Last line (inclusive).
    pub end_line: u32,
    /// Content hash of the file revision of the text (`b3:…`).
    pub content_hash: String,
    /// Evidence kind: `source_slice`.
    pub kind: String,
    /// Origin of the text: `indexed_file` (the stored copy of the indexed revision).
    pub provenance: String,
    /// Status of the symbol: `verified`, `inferred` or `candidate`.
    pub status: String,
    /// The source lines, unchanged.
    pub text: String,
    /// True when the file can differ from this revision (a change is pending, or the folder is
    /// not watched): read the range again before an edit.
    pub requires_live_read: bool,
    /// The rest of a long symbol that is not in this slice: `path:start-end`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub more: Option<String>,
}
