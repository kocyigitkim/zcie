//! Error type of `zcie-search`.

/// Errors of the search index.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// A Tantivy error.
    #[error("tantivy: {0}")]
    Tantivy(#[from] tantivy::TantivyError),
    /// A directory error.
    #[error("directory: {0}")]
    Directory(#[from] tantivy::directory::error::OpenDirectoryError),
    /// A file system error.
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    /// A storage error.
    #[error(transparent)]
    Store(#[from] zcie_store::Error),
    /// The search writer stopped.
    #[error("search writer stopped")]
    WriterStopped,
}

/// Result type of `zcie-search`.
pub type Result<T> = std::result::Result<T, Error>;
