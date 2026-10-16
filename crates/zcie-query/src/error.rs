//! Error type of `zcie-query`.

/// Errors of the read path.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The request is not valid.
    #[error("invalid request: {0}")]
    Invalid(String),
    /// The scope check failed.
    #[error("forbidden")]
    Forbidden,
    /// A record does not exist.
    #[error("not found: {0}")]
    NotFound(String),
    /// A storage error.
    #[error(transparent)]
    Store(#[from] zcie_store::Error),
    /// A graph error.
    #[error(transparent)]
    Graph(#[from] zcie_graph::Error),
    /// A symbol error.
    #[error(transparent)]
    Symbols(#[from] zcie_symbols::Error),
    /// A memory error.
    #[error(transparent)]
    Memory(#[from] zcie_memory::Error),
    /// A search error.
    #[error(transparent)]
    Search(#[from] zcie_search::Error),
}

/// Result type of `zcie-query`.
pub type Result<T> = std::result::Result<T, Error>;
