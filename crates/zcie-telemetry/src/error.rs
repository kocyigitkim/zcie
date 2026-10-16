//! Error type of `zcie-telemetry`.

/// Errors of the telemetry crate.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// A file operation failed.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

/// Result type of `zcie-telemetry`.
pub type Result<T> = std::result::Result<T, Error>;
