//! Repository, HEAD and file list.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use crate::command::run_git;
use crate::error::Result;

/// Returns true if the folder is inside a Git work tree.
pub fn is_git_repository(root: &Path) -> bool {
    toplevel(root).is_ok()
}

/// Returns the top folder of the Git work tree.
pub fn toplevel(root: &Path) -> Result<PathBuf> {
    let output = run_git(root, &["rev-parse", "--show-toplevel"])?;
    Ok(PathBuf::from(output.trim()))
}

/// Returns the HEAD commit SHA.
pub fn head_commit(root: &Path) -> Result<String> {
    Ok(run_git(root, &["rev-parse", "HEAD"])?.trim().to_string())
}

/// Returns the current branch name (`HEAD` if detached).
pub fn current_branch(root: &Path) -> Result<String> {
    Ok(run_git(root, &["rev-parse", "--abbrev-ref", "HEAD"])?.trim().to_string())
}

/// Lists tracked files and untracked files that are not ignored.
pub fn list_tracked_files(root: &Path) -> Result<Vec<String>> {
    let output = run_git(root, &["ls-files", "-z", "--cached", "--others", "--exclude-standard"])?;
    let unique: BTreeSet<String> = output
        .split('\0')
        .filter(|p| !p.is_empty())
        .map(str::to_string)
        .collect();
    Ok(unique.into_iter().collect())
}
