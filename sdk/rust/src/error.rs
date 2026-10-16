//! Error type of `zcie-sdk`.

/// Errors of the ZCIE HTTP client.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The daemon returned an error body.
    #[error("api error {status} {code}: {message}")]
    Api {
        /// HTTP status code.
        status: u16,
        /// Error code, for example `invalid_request`.
        code: String,
        /// Error message.
        message: String,
    },
    /// The HTTP request failed (connection, timeout).
    #[error("http error: {0}")]
    Http(String),
    /// The response body could not be decoded.
    #[error("decode error: {0}")]
    Decode(String),
}

/// Result type of `zcie-sdk`.
pub type Result<T> = std::result::Result<T, Error>;
