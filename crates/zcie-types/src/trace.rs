//! The query trace (ADR-019, D-066): why each candidate was kept or dropped, and how long each
//! stage took. A trace is returned only when the request asks for it; it is never logged.

use serde::{Deserialize, Serialize};

/// The trace of one query.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct QueryTrace {
    /// Query class (`search`, `task`, …).
    pub class: String,
    /// Request mode (`auto`, `locate`, …).
    pub mode: String,
    /// Selected intent (D-068), if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub intent: Option<String>,
    /// Content terms of the question (stems).
    pub terms: Vec<String>,
    /// Number of candidates of each source.
    pub sources: Vec<TraceCount>,
    /// Durations in milliseconds.
    pub stages: TraceStages,
    /// One line for each candidate, best score first.
    pub candidates: Vec<TraceCandidate>,
}

/// The candidate count of one source (`route`, `graph`).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct TraceCount {
    /// Source name.
    pub source: String,
    /// Number of candidates.
    pub count: usize,
}

/// Stage durations in milliseconds. `answer_ms` is the real answer; the other stages are measured
/// again for the trace.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct TraceStages {
    /// The whole answer (from the request to the response, with cache).
    pub answer_ms: f64,
    /// Route search.
    pub route_ms: f64,
    /// Graph ranking.
    pub graph_ms: f64,
    /// Ranking and duplicate removal.
    pub rank_ms: f64,
}

/// One candidate and the reason it was kept or dropped.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct TraceCandidate {
    /// The ref.
    #[serde(rename = "ref")]
    pub entity_ref: String,
    /// Entity kind.
    pub kind: String,
    /// Title, at most 60 characters.
    pub title: String,
    /// Source: `route`, `graph` or `answer` (a special answer such as a map).
    pub source: String,
    /// Final score after ranking (0 when the candidate was dropped before ranking).
    pub score: f32,
    /// `kept`, `echo`, `duplicate` or `below_cut`.
    pub reason: String,
}
