//! Ranking formula (`docs/09-query-and-context.md` section 5).

/// Weight of the lexical score.
pub const LEXICAL_WEIGHT: f32 = 0.6;
/// Weight of the status score.
pub const STATUS_WEIGHT: f32 = 0.2;
/// Weight of the recency score.
pub const RECENCY_WEIGHT: f32 = 0.1;
/// Weight of the kind score.
pub const KIND_WEIGHT: f32 = 0.1;

/// Returns the status weight: verified 1.0, inferred 0.6, candidate 0.3, other 0.5.
pub fn status_weight(status: Option<&str>) -> f32 {
    match status {
        Some("verified") => 1.0,
        Some("inferred") => 0.6,
        Some("candidate") => 0.3,
        _ => 0.5,
    }
}

/// Returns the recency weight `1 / (1 + age_days / 30)`.
pub fn recency_weight(age_days: f32) -> f32 {
    1.0 / (1.0 + age_days.max(0.0) / 30.0)
}

/// Returns the kind weight. `memory_first` is true for timeline and task classes; there the
/// history kinds (task, memory, commit, event) weigh 1.0 (ADR-008 I-30).
pub fn kind_weight(kind: &str, memory_first: bool) -> f32 {
    match (memory_first, kind) {
        (true, "task" | "memory" | "commit" | "event") => 1.0,
        (true, _) => 0.5,
        (false, "symbol") => 1.0,
        (false, "file") => 0.8,
        (false, _) => 0.6,
    }
}

/// Combines the parts into one score.
pub fn score(lexical_norm: f32, status: f32, recency: f32, kind: f32) -> f32 {
    LEXICAL_WEIGHT * lexical_norm + STATUS_WEIGHT * status + RECENCY_WEIGHT * recency + KIND_WEIGHT * kind
}
