//! Typed graph edges (`docs/05-data-model.md` section 7).

use serde::{Deserialize, Serialize};

use crate::ids::{EdgeId, EventId};
use crate::scope::Scope;

/// One typed, directed relation between two refs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Edge {
    /// Value schema version.
    pub v: u32,
    /// The edge ID. Stable for one `(from, relation, to)`.
    pub edge_id: EdgeId,
    /// The edge scope.
    pub scope: Scope,
    /// Source ref, for example `task:task_01...`.
    pub from: String,
    /// The relation.
    pub relation: Relation,
    /// Target ref.
    pub to: String,
    /// Trust level.
    pub status: EdgeStatus,
    /// The supporting evidence.
    pub evidence: Evidence,
    /// Confidence from 0.0 to 1.0.
    pub confidence: f32,
    /// RFC 3339 creation time.
    pub created_utc: String,
    /// Increments on each change.
    pub generation: u64,
}

/// Relation types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Relation {
    /// Task requested by a requirement.
    RequestedBy,
    /// Task implemented by a commit.
    Implements,
    /// Entity affects a file.
    AffectsFile,
    /// File defines a symbol.
    DefinesSymbol,
    /// Symbol calls a symbol.
    Calls,
    /// File imports a file.
    Imports,
    /// Entity mentions an entity.
    References,
    /// Entity tested by a test event.
    TestedBy,
    /// Entity failed in a test event.
    FailedIn,
    /// Failure fixed by an entity.
    FixedBy,
    /// File changed in a commit.
    ChangedInCommit,
    /// Record derived from an event.
    DerivedFrom,
    /// Decision made in an event.
    DecidedIn,
    /// Record contradicts a record.
    Contradicts,
    /// Record replaces a record.
    Supersedes,
    /// Entity belongs to an entity.
    BelongsTo,
    /// Weak relation.
    RelatedTo,
}

impl Relation {
    /// All relations, in declaration order.
    pub const ALL: [Relation; 17] = [
        Relation::RequestedBy,
        Relation::Implements,
        Relation::AffectsFile,
        Relation::DefinesSymbol,
        Relation::Calls,
        Relation::Imports,
        Relation::References,
        Relation::TestedBy,
        Relation::FailedIn,
        Relation::FixedBy,
        Relation::ChangedInCommit,
        Relation::DerivedFrom,
        Relation::DecidedIn,
        Relation::Contradicts,
        Relation::Supersedes,
        Relation::BelongsTo,
        Relation::RelatedTo,
    ];

    /// Returns the serialized name, for example `CHANGED_IN_COMMIT`.
    pub fn name(&self) -> &'static str {
        match self {
            Relation::RequestedBy => "REQUESTED_BY",
            Relation::Implements => "IMPLEMENTS",
            Relation::AffectsFile => "AFFECTS_FILE",
            Relation::DefinesSymbol => "DEFINES_SYMBOL",
            Relation::Calls => "CALLS",
            Relation::Imports => "IMPORTS",
            Relation::References => "REFERENCES",
            Relation::TestedBy => "TESTED_BY",
            Relation::FailedIn => "FAILED_IN",
            Relation::FixedBy => "FIXED_BY",
            Relation::ChangedInCommit => "CHANGED_IN_COMMIT",
            Relation::DerivedFrom => "DERIVED_FROM",
            Relation::DecidedIn => "DECIDED_IN",
            Relation::Contradicts => "CONTRADICTS",
            Relation::Supersedes => "SUPERSEDES",
            Relation::BelongsTo => "BELONGS_TO",
            Relation::RelatedTo => "RELATED_TO",
        }
    }

    /// Parses a serialized name.
    pub fn from_name(name: &str) -> Option<Relation> {
        Relation::ALL.into_iter().find(|r| r.name() == name)
    }
}

/// Trust level of an edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EdgeStatus {
    /// Confirmed by a reliable source.
    Verified,
    /// Found by a text or name match.
    Inferred,
    /// Possible only.
    Candidate,
    /// Rejected by a user.
    Rejected,
}

impl EdgeStatus {
    /// Returns the order of trust: Verified 3, Inferred 2, Candidate 1, Rejected 0.
    pub fn rank(&self) -> u8 {
        match self {
            EdgeStatus::Verified => 3,
            EdgeStatus::Inferred => 2,
            EdgeStatus::Candidate => 1,
            EdgeStatus::Rejected => 0,
        }
    }

    /// Returns the status with the lower rank.
    pub fn min(self, other: EdgeStatus) -> EdgeStatus {
        if other.rank() < self.rank() { other } else { self }
    }

    /// Returns the serialized name.
    pub fn name(&self) -> &'static str {
        match self {
            EdgeStatus::Verified => "verified",
            EdgeStatus::Inferred => "inferred",
            EdgeStatus::Candidate => "candidate",
            EdgeStatus::Rejected => "rejected",
        }
    }
}

/// The source that supports an edge.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Evidence {
    /// Evidence kind.
    pub kind: EvidenceKind,
    /// Source events.
    #[serde(default)]
    pub source_event_ids: Vec<EventId>,
    /// Revision of the source.
    #[serde(default)]
    pub revision: Option<String>,
    /// Short note, 200 characters or fewer.
    #[serde(default)]
    pub note: Option<String>,
}

/// The kind of evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceKind {
    /// Client sent a trace ID that joins the two entities.
    ClientTrace,
    /// The user confirmed the link.
    UserConfirmation,
    /// The commit message has `ZCIE-Task: <ref>`.
    CommitTrailer,
    /// A test result names the entity.
    TestOutcome,
    /// Git data states the relation.
    GitHistory,
    /// The engine made the record from the source event, or the user wrote an explicit ref.
    Derivation,
    /// The parser found the relation in one file.
    AstResolved,
    /// Name match across files, not resolved.
    AstApproximate,
    /// A message mentions a path or a symbol name.
    TextMention,
    /// Events occurred close in time.
    Temporal,
}

impl EvidenceKind {
    /// Returns the maximum edge status for this evidence (`docs/05-data-model.md` section 7.1).
    pub fn max_status(&self) -> EdgeStatus {
        match self {
            EvidenceKind::ClientTrace
            | EvidenceKind::UserConfirmation
            | EvidenceKind::CommitTrailer
            | EvidenceKind::AstResolved
            | EvidenceKind::TestOutcome
            | EvidenceKind::GitHistory
            | EvidenceKind::Derivation => EdgeStatus::Verified,
            EvidenceKind::TextMention | EvidenceKind::AstApproximate => EdgeStatus::Inferred,
            EvidenceKind::Temporal => EdgeStatus::Candidate,
        }
    }

    /// Returns the serialized name.
    pub fn name(&self) -> &'static str {
        match self {
            EvidenceKind::ClientTrace => "client_trace",
            EvidenceKind::UserConfirmation => "user_confirmation",
            EvidenceKind::CommitTrailer => "commit_trailer",
            EvidenceKind::TestOutcome => "test_outcome",
            EvidenceKind::GitHistory => "git_history",
            EvidenceKind::Derivation => "derivation",
            EvidenceKind::AstResolved => "ast_resolved",
            EvidenceKind::AstApproximate => "ast_approximate",
            EvidenceKind::TextMention => "text_mention",
            EvidenceKind::Temporal => "temporal",
        }
    }
}
