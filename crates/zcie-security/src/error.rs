//! Error type of `zcie-security`.

/// Errors of the security crate.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The API key is missing or wrong.
    #[error("unauthorized")]
    Unauthorized,
    /// The principal has no permission.
    #[error("forbidden")]
    Forbidden,
    /// A file does not exist.
    #[error("not found: {0}")]
    NotFound(String),
    /// A file operation failed.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    /// A TOML file has the wrong form.
    #[error("toml error: {0}")]
    Toml(String),
    /// A glob pattern has the wrong form.
    #[error("glob error: {0}")]
    Glob(String),
    /// The random-number source failed.
    #[error("random error: {0}")]
    Random(String),
}

/// Result type of `zcie-security`.
pub type Result<T> = std::result::Result<T, Error>;
