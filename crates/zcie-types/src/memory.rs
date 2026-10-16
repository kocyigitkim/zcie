//! Structured memory records (`docs/05-data-model.md` section 4).

use serde::{Deserialize, Serialize};

use crate::ids::{EventId, MemoryId};
use crate::scope::Scope;

/// One structured memory item derived from raw events.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MemoryRecord {
    /// Value schema version.
    pub v: u32,
    /// The record ID.
    pub memory_id: MemoryId,
    /// The record scope.
    pub scope: Scope,
    /// The record kind.
    pub kind: MemoryKind,
    /// Short statement, 500 characters or fewer.
    pub text: String,
    /// The record status.
    pub status: MemoryStatus,
    /// Confidence from 0.0 to 1.0.
    pub confidence: f32,
    /// Source events. Never empty.
    pub source_event_ids: Vec<EventId>,
    /// RFC 3339 start of validity.
    pub valid_from: String,
    /// RFC 3339 end of validity.
    #[serde(default)]
    pub valid_until: Option<String>,
    /// Last revision that verified the record.
    #[serde(default)]
    pub last_verified_revision: Option<String>,
    /// The record that replaces this record.
    #[serde(default)]
    pub superseded_by: Option<MemoryId>,
    /// Extractor name, for example `tier_a.v1` or `manual`.
    pub extractor: String,
    /// Increments on each change.
    pub generation: u64,
}

/// The kind of a memory record.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryKind {
    /// A verified or attributed statement.
    Fact,
    /// Something the user requested.
    UserRequirement,
    /// A rule or a limit.
    Constraint,
    /// An accepted decision.
    DesignDecision,
    /// A work item.
    Task,
    /// A log, failure or tool result.
    Observation,
    /// An unverified explanation.
    Hypothesis,
    /// A correction of earlier information.
    Correction,
    /// A user working preference.
    Preference,
}

/// The status of a memory record.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryStatus {
    /// Active, not verified.
    Active,
    /// Verified.
    Verified,
    /// Disputed by a contradiction.
    Disputed,
    /// Replaced by a correction.
    Superseded,
    /// Removed by `forget`.
    Retracted,
}

impl MemoryKind {
    /// Returns the serialized name.
    pub fn name(&self) -> &'static str {
        match self {
            MemoryKind::Fact => "fact",
            MemoryKind::UserRequirement => "user_requirement",
            MemoryKind::Constraint => "constraint",
            MemoryKind::DesignDecision => "design_decision",
            MemoryKind::Task => "task",
            MemoryKind::Observation => "observation",
            MemoryKind::Hypothesis => "hypothesis",
            MemoryKind::Correction => "correction",
            MemoryKind::Preference => "preference",
        }
    }
}

impl MemoryStatus {
    /// Returns the serialized name.
    pub fn name(&self) -> &'static str {
        match self {
            MemoryStatus::Active => "active",
            MemoryStatus::Verified => "verified",
            MemoryStatus::Disputed => "disputed",
            MemoryStatus::Superseded => "superseded",
            MemoryStatus::Retracted => "retracted",
        }
    }
}
