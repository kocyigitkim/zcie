//! Error type of `zcie-symbols`.

/// Errors of the symbol index.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// A storage error.
    #[error(transparent)]
    Store(#[from] zcie_store::Error),
}

/// Result type of `zcie-symbols`.
pub type Result<T> = std::result::Result<T, Error>;
