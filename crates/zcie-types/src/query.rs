//! Query and expand types (`docs/09-query-and-context.md` sections 2 and 6.1).

use serde::{Deserialize, Serialize};

/// A query request (HTTP form).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct QueryRequest {
    /// Workspace name or `ws_` ID.
    pub workspace: String,
    /// Optional repository limit.
    #[serde(default)]
    pub repository_id: Option<String>,
    /// Question, symbol name or ref. 2000 characters or fewer.
    pub question: String,
    /// Query mode. Default `auto`.
    #[serde(default)]
    pub mode: QueryMode,
    /// Token budget. Default `query.default_token_budget`.
    #[serde(default)]
    pub token_budget: Option<u32>,
    /// Maximum results. Default `query.max_results`.
    #[serde(default)]
    pub max_results: Option<u32>,
    /// Continuation from an earlier response.
    #[serde(default)]
    pub cursor: Option<String>,
    /// `project` (default): the project in `workspace`. `global`: all permitted projects.
    #[serde(default)]
    pub scope: QueryScope,
    /// Also query other registered projects whose name appears in the question.
    #[serde(default)]
    pub cross_project: bool,
    /// Add the trace of the query to the response (ADR-019, D-066).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub explain: bool,
}

/// Which projects a query reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QueryScope {
    /// One project.
    #[default]
    Project,
    /// All projects that the API key permits.
    Global,
}

/// Query mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QueryMode {
    /// Classify the question with rules.
    #[default]
    Auto,
    /// Exact lookup only.
    Exact,
    /// Lexical search.
    Search,
    /// Timeline and memory.
    Timeline,
    /// Memory records.
    Memory,
    /// Repository map: the most important files and symbols (ADR-017).
    Map,
    /// Files, symbols and line ranges to change for a request (ADR-017).
    Locate,
}

impl QueryMode {
    /// Returns the serialized name.
    pub fn name(&self) -> &'static str {
        match self {
            QueryMode::Auto => "auto",
            QueryMode::Exact => "exact",
            QueryMode::Search => "search",
            QueryMode::Timeline => "timeline",
            QueryMode::Memory => "memory",
            QueryMode::Map => "map",
            QueryMode::Locate => "locate",
        }
    }
}

/// Query class from the classifier (`docs/09-query-and-context.md` section 3.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QueryClass {
    /// The question is a ref.
    ExactRef,
    /// Where is a symbol defined.
    SymbolDefinition,
    /// Who calls a symbol.
    Callers,
    /// History and timeline.
    Timeline,
    /// Which commit implemented a request.
    Implementation,
    /// The question has a task ref.
    Task,
    /// Any other question.
    Search,
}

/// A query response.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QueryResponse {
    /// `q_<ulid>`.
    pub query_id: String,
    /// The query class.
    pub class: QueryClass,
    /// The answer kind.
    pub answer_kind: AnswerKind,
    /// Short local answer, only for `AnswerKind::Local`.
    pub answer: Option<String>,
    /// Ranked results.
    pub results: Vec<ResultItem>,
    /// Related task and memory refs.
    pub memory_links: Vec<String>,
    /// Freshness label.
    pub freshness: Freshness,
    /// Watermarks of the snapshot.
    pub watermarks: Watermarks,
    /// Token budget that applied.
    pub token_budget: u32,
    /// Estimated tokens of the response.
    pub token_estimate: u32,
    /// True if results were left out.
    pub truncated: bool,
    /// Continuation cursor.
    pub cursor: Option<String>,
    /// Short reason for `not_found` or partial answers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    /// Best share of the question terms that one result contains (0.0 to 1.0).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub match_coverage: Option<f32>,
    /// `false`: the prompt names no project fact and no result answers it, so clients add no
    /// context capsule (ADR-017). Absent: the clients decide with their usual gate. Only `false`
    /// is sent (clients check `capsule != false`, so `true` would only cost tokens).
    #[serde(default, skip_serializing_if = "not_false")]
    pub capsule: Option<bool>,
    /// The next action that the prompt asks for (ADR-019, D-068). `general` is not sent (it is the
    /// default and would only cost tokens).
    #[serde(default, skip_serializing_if = "crate::evidence::is_general")]
    pub intent: Option<crate::evidence::QueryIntent>,
    /// Exact source slices for an edit, explain or trace prompt (ADR-019, D-069).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub evidence: Vec<crate::evidence::EvidenceItem>,
    /// The query trace, only when the request has `explain: true` (ADR-019, D-066).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trace: Option<crate::trace::QueryTrace>,
}

/// The answer kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnswerKind {
    /// A short answer from verified data.
    Local,
    /// Cited evidence only.
    Evidence,
    /// Nothing found.
    NotFound,
}

/// One result item.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResultItem {
    /// The entity ref.
    #[serde(rename = "ref")]
    pub entity_ref: String,
    /// `symbol`, `file`, `memory`, `task`, `commit`, `event`.
    pub kind: String,
    /// Qualified name, path, task title or memory text.
    pub title: String,
    /// File path, if any.
    pub path: Option<String>,
    /// Line range, if any.
    pub range: Option<[u32; 2]>,
    /// Edge path status or record status.
    pub status: Option<String>,
    /// Evidence label, for example `verified_ast`.
    pub evidence_status: Option<String>,
    /// Revision of the entity.
    pub revision: Option<String>,
    /// Ranking score.
    pub score: f32,
    /// Snippet (level L1 and higher).
    pub snippet: Option<String>,
    /// Project name, set when a query reads more than one project.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
    /// For a memory record of a follow-up message: the user message before it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context: Option<String>,
    /// For a top user message: the next short user messages of its session, which often refine
    /// it ("bunu otomatik yapsın ama maliyet artmasın").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub follow_up: Option<String>,
}

/// Freshness label of a response.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Freshness {
    /// All indexes are current.
    Current,
    /// A result could not be confirmed.
    Stale,
    /// An index is behind; results were checked where possible.
    Partial,
}

/// Index watermarks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Watermarks {
    /// Highest durable event seq.
    pub canonical_seq: u64,
    /// All jobs with seq at or below this value are complete.
    pub graph_seq: u64,
    /// Tantivy has committed documents up to this seq.
    pub search_seq: u64,
}

/// An expand request (HTTP form).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct ExpandRequest {
    /// Workspace name or `ws_` ID.
    pub workspace: String,
    /// 1 to 20 refs. An item can also be a line range `path:start-end` (at most 10 ranges).
    pub refs: Vec<String>,
    /// Expansion level. Default `evidence`.
    #[serde(default)]
    pub level: ExpandLevel,
    /// Token budget.
    #[serde(default)]
    pub token_budget: Option<u32>,
    /// Expected revision of the refs.
    #[serde(default)]
    pub expected_revision: Option<String>,
    /// Related code to add to each symbol ref: `callers`, `tests` (ADR-017, D-063).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub with: Vec<String>,
}

/// Expansion level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExpandLevel {
    /// L1: facts, snippets, provenance.
    #[default]
    Evidence,
    /// L2: full symbol bodies.
    Source,
    /// L3: original events and full files.
    Audit,
}

/// An expand response.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExpandResponse {
    /// One item per ref.
    pub items: Vec<ExpandItem>,
    /// Freshness label.
    pub freshness: Freshness,
    /// Estimated tokens.
    pub token_estimate: u32,
    /// True if items were left out.
    pub truncated: bool,
    /// Continuation cursor.
    pub cursor: Option<String>,
}

/// One expanded ref.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExpandItem {
    /// The ref.
    #[serde(rename = "ref")]
    pub entity_ref: String,
    /// Expansion status.
    pub status: ExpandStatus,
    /// Revision of the content.
    pub revision: Option<String>,
    /// File path, if any.
    #[serde(default)]
    pub path: Option<String>,
    /// Line range, if any.
    #[serde(default)]
    pub range: Option<[u32; 2]>,
    /// The content.
    pub content: Option<String>,
    /// Source event refs.
    pub provenance: Vec<String>,
    /// The rest of a cut range: `path:start-end` (ADR-017, D-063).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub more: Option<String>,
    /// Related lines (`with`): "caller path:line text" or "test path:line text".
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub related: Vec<String>,
}

/// Expansion status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExpandStatus {
    /// Content is current.
    Ok,
    /// The source changed since indexing.
    Changed,
    /// The ref does not exist.
    NotFound,
    /// The scope check failed.
    Forbidden,
}

/// True unless the value is `Some(false)` (the serde skip rule of `QueryResponse::capsule`).
fn not_false(value: &Option<bool>) -> bool {
    *value != Some(false)
}
