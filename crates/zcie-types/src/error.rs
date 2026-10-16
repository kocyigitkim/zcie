//! Error type of `zcie-types`.

/// Errors of the shared types.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// An ID or a ref has the wrong form.
    #[error("invalid id: {0}")]
    InvalidId(String),
    /// A value has the wrong form.
    #[error("invalid value: {0}")]
    InvalidValue(String),
}

/// Result type of `zcie-types`.
pub type Result<T> = std::result::Result<T, Error>;
