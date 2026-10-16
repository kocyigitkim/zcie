//! Error type of `zcie-git`.

/// Errors of the Git crate.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The `git` program is not installed.
    #[error("git is not available")]
    GitUnavailable,
    /// A `git` command failed.
    #[error("git command failed: {0}")]
    Command(String),
    /// The output had an unexpected form.
    #[error("unexpected git output: {0}")]
    Parse(String),
    /// A process operation failed.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

/// Result type of `zcie-git`.
pub type Result<T> = std::result::Result<T, Error>;
