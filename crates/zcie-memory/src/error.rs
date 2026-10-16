//! Error type of `zcie-memory`.

/// Errors of the memory layer.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// A storage error.
    #[error(transparent)]
    Store(#[from] zcie_store::Error),
    /// A graph error.
    #[error(transparent)]
    Graph(#[from] zcie_graph::Error),
    /// A symbol index error.
    #[error(transparent)]
    Symbols(#[from] zcie_symbols::Error),
    /// A record does not exist.
    #[error("not found: {0}")]
    NotFound(String),
    /// A request is not valid.
    #[error("invalid: {0}")]
    Invalid(String),
}

/// Result type of `zcie-memory`.
pub type Result<T> = std::result::Result<T, Error>;
