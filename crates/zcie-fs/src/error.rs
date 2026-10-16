//! Error type of `zcie-fs`.

/// Errors of the file crate.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// A file operation failed.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    /// The file watcher failed.
    #[error("watcher error: {0}")]
    Notify(String),
    /// The folder walk failed.
    #[error("walk error: {0}")]
    Walk(String),
}

/// Result type of `zcie-fs`.
pub type Result<T> = std::result::Result<T, Error>;
