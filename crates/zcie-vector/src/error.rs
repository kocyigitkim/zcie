//! The error type of `zcie-vector`.

/// A vector error.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// A model file cannot be read.
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    /// A model file has a wrong format.
    #[error("model format: {0}")]
    Format(String),
}

/// The result type of `zcie-vector`.
pub type Result<T> = std::result::Result<T, Error>;
