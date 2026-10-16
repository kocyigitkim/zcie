//! Git data from the `git` command-line program (ADR-004).

pub mod command;
pub mod commits;
pub mod error;
pub mod head;
pub mod trailers;

pub use command::run_git;
pub use commits::{ChangedPath, CommitInfo, commit_info, commits_since, recent_commits};
pub use error::{Error, Result};
pub use head::{current_branch, head_commit, is_git_repository, list_tracked_files, toplevel};
pub use trailers::{parse_trailers, task_trailers};
