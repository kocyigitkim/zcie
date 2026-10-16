//! Error type of `zcie-core`.

/// Errors of engine startup.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The configuration is not valid.
    #[error("configuration: {0}")]
    Config(String),
    /// The instance manifest is not supported.
    #[error("manifest: {0}")]
    Manifest(String),
    /// A file system error.
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    /// A storage error.
    #[error(transparent)]
    Store(#[from] zcie_store::Error),
    /// A search error.
    #[error(transparent)]
    Search(#[from] zcie_search::Error),
    /// A security error.
    #[error(transparent)]
    Security(#[from] zcie_security::Error),
    /// A runtime error.
    #[error(transparent)]
    Runtime(#[from] zcie_runtime::Error),
    /// A write path error.
    #[error(transparent)]
    Ingest(#[from] zcie_ingest::Error),
}

/// Result type of `zcie-core`.
pub type Result<T> = std::result::Result<T, Error>;
