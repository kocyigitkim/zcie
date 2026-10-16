//! Error type of `zcie-store`.

/// Errors of the storage layer.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// A RocksDB error.
    #[error("rocksdb: {0}")]
    Rocks(#[from] rocksdb::Error),
    /// A file system error.
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    /// A value could not be encoded or decoded.
    #[error("codec: {0}")]
    Codec(#[from] serde_json::Error),
    /// A column family is missing.
    #[error("missing column family: {0}")]
    MissingCf(&'static str),
    /// A blob does not agree with its hash.
    #[error("blob hash mismatch: {0}")]
    HashMismatch(String),
    /// A blob or a value does not exist.
    #[error("not found: {0}")]
    NotFound(String),
    /// A value has the wrong form.
    #[error("invalid: {0}")]
    Invalid(String),
}

/// Result type of `zcie-store`.
pub type Result<T> = std::result::Result<T, Error>;
