//! Error type of `zcie-runtime`.

/// Errors of the runtime.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The queue is full.
    #[error("queue is full")]
    Busy,
    /// The runtime is shutting down.
    #[error("shutting down")]
    ShuttingDown,
    /// A worker thread could not start.
    #[error("worker start failed: {0}")]
    Spawn(String),
}

/// Result type of `zcie-runtime`.
pub type Result<T> = std::result::Result<T, Error>;
