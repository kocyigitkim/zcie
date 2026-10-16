//! Commit lists and commit data.

use std::path::Path;

use crate::command::run_git;
use crate::error::{Error, Result};
use crate::trailers::parse_trailers;

/// One changed path of a commit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangedPath {
    /// Status letter(s), for example `M`, `A`, `D`, `R100`.
    pub status: String,
    /// The path after the commit.
    pub path: String,
    /// The old path of a rename or a copy.
    pub old_path: Option<String>,
}

/// Data of one commit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitInfo {
    /// Full SHA.
    pub sha: String,
    /// Author name.
    pub author: String,
    /// RFC 3339 author time.
    pub author_utc: String,
    /// Full message.
    pub message: String,
    /// Changed paths.
    pub changed: Vec<ChangedPath>,
    /// Trailers of the message.
    pub trailers: Vec<(String, String)>,
}

/// Returns the SHAs of the last `count` commits, newest first.
pub fn recent_commits(root: &Path, count: usize) -> Result<Vec<String>> {
    let count = count.to_string();
    let output = run_git(root, &["log", "--format=%H", "-n", &count])?;
    Ok(lines(&output))
}

/// Returns the SHAs of the commits after `old` up to HEAD, newest first.
pub fn commits_since(root: &Path, old: &str) -> Result<Vec<String>> {
    let range = format!("{old}..HEAD");
    let output = run_git(root, &["log", "--format=%H", &range])?;
    Ok(lines(&output))
}

/// Reads the data and the changed paths of one commit.
pub fn commit_info(root: &Path, sha: &str) -> Result<CommitInfo> {
    let output = run_git(root, &["show", "-s", "--format=%H%x00%an%x00%aI%x00%B", sha])?;
    let mut parts = output.splitn(4, '\0');
    let full_sha = parts.next().unwrap_or_default().trim().to_string();
    let author = parts.next().unwrap_or_default().to_string();
    let author_utc = parts.next().unwrap_or_default().to_string();
    let message = parts.next().unwrap_or_default().trim_end().to_string();
    if full_sha.is_empty() {
        return Err(Error::Parse(format!("no commit data for {sha}")));
    }
    let changed = changed_paths(root, &full_sha)?;
    let trailers = parse_trailers(&message);
    Ok(CommitInfo { sha: full_sha, author, author_utc, message, changed, trailers })
}

fn changed_paths(root: &Path, sha: &str) -> Result<Vec<ChangedPath>> {
    let args = ["diff-tree", "--root", "--no-commit-id", "-r", "--name-status", "-M", sha];
    let output = run_git(root, &args)?;
    Ok(output.lines().filter_map(parse_name_status).collect())
}

fn parse_name_status(line: &str) -> Option<ChangedPath> {
    let fields: Vec<&str> = line.split('\t').collect();
    match fields.as_slice() {
        [status, path] => Some(ChangedPath {
            status: status.to_string(),
            path: path.to_string(),
            old_path: None,
        }),
        [status, old, new] => Some(ChangedPath {
            status: status.to_string(),
            path: new.to_string(),
            old_path: Some(old.to_string()),
        }),
        _ => None,
    }
}

fn lines(output: &str) -> Vec<String> {
    output.lines().map(str::trim).filter(|l| !l.is_empty()).map(str::to_string).collect()
}
