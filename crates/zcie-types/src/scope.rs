//! The isolation scope of a request or a record (`docs/05-data-model.md` section 2).

use serde::{Deserialize, Serialize};

/// The tenant of milestone 1 (fixed decision F-10).
pub const LOCAL_TENANT: &str = "local";

/// The isolation scope of a request or a record.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct Scope {
    /// Tenant ID. Always `local` in milestone 1.
    pub tenant_id: String,
    /// Principal that owns or sent the record.
    pub principal_id: String,
    /// Workspace ID (`ws_...`).
    pub workspace_id: String,
    /// Optional project ID.
    #[serde(default)]
    pub project_id: Option<String>,
    /// Optional repository ID.
    #[serde(default)]
    pub repository_id: Option<String>,
    /// Optional worktree ID.
    #[serde(default)]
    pub worktree_id: Option<String>,
}

impl Scope {
    /// Makes a workspace-level scope in the local tenant.
    pub fn workspace(principal_id: &str, workspace_id: &str) -> Self {
        Self {
            tenant_id: LOCAL_TENANT.to_string(),
            principal_id: principal_id.to_string(),
            workspace_id: workspace_id.to_string(),
            project_id: None,
            repository_id: None,
            worktree_id: None,
        }
    }

    /// Returns true if `other` is inside this scope.
    ///
    /// Tenant and workspace must be equal. Each optional field that this scope sets must be equal in `other`.
    pub fn contains(&self, other: &Scope) -> bool {
        self.tenant_id == other.tenant_id
            && self.workspace_id == other.workspace_id
            && optional_matches(&self.project_id, &other.project_id)
            && optional_matches(&self.repository_id, &other.repository_id)
            && optional_matches(&self.worktree_id, &other.worktree_id)
    }
}

fn optional_matches(limit: &Option<String>, value: &Option<String>) -> bool {
    match limit {
        None => true,
        Some(limit) => value.as_ref() == Some(limit),
    }
}

/// Returns the current UTC time in RFC 3339 format.
pub fn now_utc() -> String {
    let now = time::OffsetDateTime::now_utc();
    now.format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| now.unix_timestamp().to_string())
}

/// Parses an RFC 3339 time. Returns seconds since the Unix epoch.
pub fn parse_utc_seconds(value: &str) -> Option<i64> {
    time::OffsetDateTime::parse(value, &time::format_description::well_known::Rfc3339)
        .ok()
        .map(|t| t.unix_timestamp())
}
