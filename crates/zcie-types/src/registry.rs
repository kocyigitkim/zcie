//! Registry records (`docs/06-storage.md` section 3.1).

use serde::{Deserialize, Serialize};

/// A named workspace of one principal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceRecord {
    /// Value schema version.
    pub v: u32,
    /// `ws_...`.
    pub workspace_id: String,
    /// Human name, for example `atheron`.
    pub name: String,
    /// Owner principal.
    pub principal_id: String,
    /// RFC 3339 creation time.
    pub created_utc: String,
}

/// A logical product inside a workspace.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectRecord {
    /// Value schema version.
    pub v: u32,
    /// `proj_...`.
    pub project_id: String,
    /// Workspace ID.
    pub workspace_id: String,
    /// Project name.
    pub name: String,
}

/// One Git repository or plain folder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepositoryRecord {
    /// Value schema version.
    pub v: u32,
    /// `repo_...`.
    pub repository_id: String,
    /// Workspace ID.
    pub workspace_id: String,
    /// Optional project ID.
    #[serde(default)]
    pub project_id: Option<String>,
    /// Repository name (folder name).
    pub name: String,
    /// True if Git data is available.
    pub has_git: bool,
}

/// One checked-out folder of a repository.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorktreeRecord {
    /// Value schema version.
    pub v: u32,
    /// `wt_...`.
    pub worktree_id: String,
    /// Repository ID.
    pub repository_id: String,
    /// Workspace ID.
    #[serde(default)]
    pub workspace_id: String,
    /// Absolute root path.
    pub root_path: String,
    /// HEAD at the last scan.
    #[serde(default)]
    pub last_scanned_head: Option<String>,
    /// RFC 3339 time of the last scan.
    #[serde(default)]
    pub last_scan_utc: Option<String>,
    /// True if the files of the folder are indexed and watched (`zcie project add`, `zcie repo
    /// add`). False: the folder only maps conversations to its project (a folder of an imported
    /// conversation or of a Claude Code session); no scan, no watcher (ADR-015).
    #[serde(default)]
    pub indexed: bool,
}
