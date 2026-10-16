//! Error type of `zcie-graph`.

/// Errors of the graph layer.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// A storage error.
    #[error(transparent)]
    Store(#[from] zcie_store::Error),
}

/// Result type of `zcie-graph`.
pub type Result<T> = std::result::Result<T, Error>;
