//! Error type of `zcie-ingest`.

/// Errors of the write path.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The request is not valid.
    #[error("invalid request: {0}")]
    Invalid(String),
    /// The capture policy rejects the event.
    #[error("forbidden: {0}")]
    Forbidden(String),
    /// The payload is larger than the limit.
    #[error("payload too large")]
    PayloadTooLarge,
    /// A record does not exist.
    #[error("not found: {0}")]
    NotFound(String),
    /// A storage error.
    #[error(transparent)]
    Store(#[from] zcie_store::Error),
    /// A memory error.
    #[error(transparent)]
    Memory(#[from] zcie_memory::Error),
    /// A graph error.
    #[error(transparent)]
    Graph(#[from] zcie_graph::Error),
    /// A symbol error.
    #[error(transparent)]
    Symbols(#[from] zcie_symbols::Error),
    /// A search error.
    #[error(transparent)]
    Search(#[from] zcie_search::Error),
    /// A file system error.
    #[error(transparent)]
    Fs(#[from] zcie_fs::Error),
    /// A Git error.
    #[error(transparent)]
    Git(#[from] zcie_git::Error),
    /// An I/O error.
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// Result type of `zcie-ingest`.
pub type Result<T> = std::result::Result<T, Error>;
