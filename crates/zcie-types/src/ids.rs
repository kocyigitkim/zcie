//! Typed entity IDs (`docs/05-data-model.md` section 1) and refs.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

/// Makes a new ID string: the prefix followed by a new ULID.
pub fn new_id(prefix: &str) -> String {
    format!("{prefix}{}", ulid::Ulid::generate())
}

/// Checks that `value` starts with `prefix` and has a non-empty rest.
pub fn check_prefix(value: &str, prefix: &str) -> Result<()> {
    match value.strip_prefix(prefix) {
        Some(rest) if !rest.is_empty() => Ok(()),
        _ => Err(Error::InvalidId(format!("expected prefix {prefix}: {value}"))),
    }
}

/// ID of an event (`ev_`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct EventId(pub String);

impl EventId {
    /// Prefix of this ID kind.
    pub const PREFIX: &'static str = "ev_";
    /// Makes a new unique ID.
    pub fn new() -> Self {
        Self(new_id(Self::PREFIX))
    }
    /// Parses an ID and checks its prefix.
    pub fn parse(value: &str) -> Result<Self> {
        check_prefix(value, Self::PREFIX).map(|_| Self(value.to_string()))
    }
    /// Returns the ID as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for EventId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for EventId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// ID of a file record (`file_`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct FileId(pub String);

impl FileId {
    /// Prefix of this ID kind.
    pub const PREFIX: &'static str = "file_";
    /// Makes a new unique ID.
    pub fn new() -> Self {
        Self(new_id(Self::PREFIX))
    }
    /// Parses an ID and checks its prefix.
    pub fn parse(value: &str) -> Result<Self> {
        check_prefix(value, Self::PREFIX).map(|_| Self(value.to_string()))
    }
    /// Returns the ID as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for FileId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for FileId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// ID of a symbol record (`sym_`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SymbolId(pub String);

impl SymbolId {
    /// Prefix of this ID kind.
    pub const PREFIX: &'static str = "sym_";
    /// Makes a new unique ID.
    pub fn new() -> Self {
        Self(new_id(Self::PREFIX))
    }
    /// Parses an ID and checks its prefix.
    pub fn parse(value: &str) -> Result<Self> {
        check_prefix(value, Self::PREFIX).map(|_| Self(value.to_string()))
    }
    /// Returns the ID as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for SymbolId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for SymbolId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// ID of a memory record (`mem_`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct MemoryId(pub String);

impl MemoryId {
    /// Prefix of this ID kind.
    pub const PREFIX: &'static str = "mem_";
    /// Makes a new unique ID.
    pub fn new() -> Self {
        Self(new_id(Self::PREFIX))
    }
    /// Parses an ID and checks its prefix.
    pub fn parse(value: &str) -> Result<Self> {
        check_prefix(value, Self::PREFIX).map(|_| Self(value.to_string()))
    }
    /// Returns the ID as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for MemoryId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for MemoryId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// ID of a task (`task_`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TaskId(pub String);

impl TaskId {
    /// Prefix of this ID kind.
    pub const PREFIX: &'static str = "task_";
    /// Makes a new unique ID.
    pub fn new() -> Self {
        Self(new_id(Self::PREFIX))
    }
    /// Parses an ID and checks its prefix.
    pub fn parse(value: &str) -> Result<Self> {
        check_prefix(value, Self::PREFIX).map(|_| Self(value.to_string()))
    }
    /// Returns the ID as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for TaskId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for TaskId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// ID of an edge (`edge_`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct EdgeId(pub String);

impl EdgeId {
    /// Prefix of this ID kind.
    pub const PREFIX: &'static str = "edge_";
    /// Makes a new unique ID.
    pub fn new() -> Self {
        Self(new_id(Self::PREFIX))
    }
    /// Parses an ID and checks its prefix.
    pub fn parse(value: &str) -> Result<Self> {
        check_prefix(value, Self::PREFIX).map(|_| Self(value.to_string()))
    }
    /// Returns the ID as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for EdgeId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for EdgeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// ID of a job (`job_`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct JobId(pub String);

impl JobId {
    /// Prefix of this ID kind.
    pub const PREFIX: &'static str = "job_";
    /// Makes a new unique ID.
    pub fn new() -> Self {
        Self(new_id(Self::PREFIX))
    }
    /// Parses an ID and checks its prefix.
    pub fn parse(value: &str) -> Result<Self> {
        check_prefix(value, Self::PREFIX).map(|_| Self(value.to_string()))
    }
    /// Returns the ID as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for JobId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for JobId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// ID of a commit (`commit_<sha>`). It contains the Git SHA, not a ULID.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CommitId(pub String);

impl CommitId {
    /// Prefix of this ID kind.
    pub const PREFIX: &'static str = "commit_";
    /// Makes the commit ID for a Git SHA.
    pub fn from_sha(sha: &str) -> Self {
        Self(format!("{}{sha}", Self::PREFIX))
    }
    /// Parses an ID and checks its prefix.
    pub fn parse(value: &str) -> Result<Self> {
        check_prefix(value, Self::PREFIX).map(|_| Self(value.to_string()))
    }
    /// Returns the Git SHA part of the ID.
    pub fn sha(&self) -> &str {
        self.0.strip_prefix(Self::PREFIX).unwrap_or(&self.0)
    }
    /// Returns the ID as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for CommitId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Registry ID prefixes. Registry IDs are plain strings in records.
pub mod prefix {
    /// Workspace ID prefix.
    pub const WORKSPACE: &str = "ws_";
    /// Project ID prefix.
    pub const PROJECT: &str = "proj_";
    /// Repository ID prefix.
    pub const REPOSITORY: &str = "repo_";
    /// Worktree ID prefix.
    pub const WORKTREE: &str = "wt_";
    /// Query ID prefix.
    pub const QUERY: &str = "q_";
}

/// Returns true for a project ID: `proj_<ULID>`, or `ws_<ULID>` of projects made before project IDs.
pub fn is_project_id(value: &str) -> bool {
    let rest = value.strip_prefix(prefix::PROJECT).or_else(|| value.strip_prefix(prefix::WORKSPACE));
    rest.is_some_and(|r| r.len() == 26 && r.chars().all(|c| c.is_ascii_alphanumeric()))
}

/// The permitted ref kinds (`docs/08-memory-and-graph.md` section 2).
pub const REF_KINDS: [&str; 6] = ["ev", "file", "sym", "mem", "task", "commit"];

/// A short handle to an entity, in the form `<kind>:<id>`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Ref {
    /// The ref kind, for example `sym`.
    pub kind: String,
    /// The entity ID, for example `sym_01J...`.
    pub id: String,
}

impl Ref {
    /// Makes a ref from a kind and an ID.
    pub fn new(kind: &str, id: &str) -> Self {
        Self { kind: kind.to_string(), id: id.to_string() }
    }

    /// Parses `<kind>:<id>`. The kind must be in `REF_KINDS`.
    pub fn parse(value: &str) -> Result<Self> {
        let (kind, id) = value
            .split_once(':')
            .ok_or_else(|| Error::InvalidId(format!("not a ref: {value}")))?;
        if !REF_KINDS.contains(&kind) || id.is_empty() {
            return Err(Error::InvalidId(format!("not a ref: {value}")));
        }
        Ok(Self::new(kind, id))
    }
}

impl fmt::Display for Ref {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.kind, self.id)
    }
}
