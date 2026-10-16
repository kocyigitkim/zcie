//! `ApiError` (`docs/10-api-and-gateways.md` section 2.1).

/// Errors of the core API. Each error has an HTTP status and a code.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ApiError {
    /// The API key is missing or wrong.
    #[error("unauthorized")]
    Unauthorized,
    /// The principal has no permission for the operation or the scope.
    #[error("forbidden: {0}")]
    Forbidden(String),
    /// The entity does not exist.
    #[error("not found: {0}")]
    NotFound(String),
    /// The request is not valid.
    #[error("invalid request: {0}")]
    InvalidRequest(String),
    /// The payload is larger than the limit.
    #[error("payload too large")]
    PayloadTooLarge,
    /// The queue or the disk is saturated.
    #[error("busy")]
    Busy,
    /// An internal error. The message has no paths or secrets.
    #[error("internal error: {0}")]
    Internal(String),
}

impl ApiError {
    /// Returns the machine-readable code.
    pub fn code(&self) -> &'static str {
        match self {
            ApiError::Unauthorized => "unauthorized",
            ApiError::Forbidden(_) => "forbidden",
            ApiError::NotFound(_) => "not_found",
            ApiError::InvalidRequest(_) => "invalid_request",
            ApiError::PayloadTooLarge => "payload_too_large",
            ApiError::Busy => "busy",
            ApiError::Internal(_) => "internal",
        }
    }

    /// Returns the HTTP status code.
    pub fn http_status(&self) -> u16 {
        match self {
            ApiError::Unauthorized => 401,
            ApiError::Forbidden(_) => 403,
            ApiError::NotFound(_) => 404,
            ApiError::InvalidRequest(_) => 400,
            ApiError::PayloadTooLarge => 413,
            ApiError::Busy => 503,
            ApiError::Internal(_) => 500,
        }
    }

    /// Makes an internal error and logs the detail.
    pub fn internal(error: impl std::fmt::Display) -> ApiError {
        tracing::error!(%error, "internal error");
        ApiError::Internal("see daemon log".into())
    }
}

impl From<zcie_ingest::Error> for ApiError {
    fn from(error: zcie_ingest::Error) -> Self {
        match error {
            zcie_ingest::Error::Invalid(m) => ApiError::InvalidRequest(m),
            zcie_ingest::Error::Forbidden(m) => ApiError::Forbidden(m),
            zcie_ingest::Error::PayloadTooLarge => ApiError::PayloadTooLarge,
            zcie_ingest::Error::NotFound(m) => ApiError::NotFound(m),
            other => ApiError::internal(other),
        }
    }
}

impl From<zcie_query::Error> for ApiError {
    fn from(error: zcie_query::Error) -> Self {
        match error {
            zcie_query::Error::Invalid(m) => ApiError::InvalidRequest(m),
            zcie_query::Error::Forbidden => ApiError::Forbidden("scope".into()),
            zcie_query::Error::NotFound(m) => ApiError::NotFound(m),
            other => ApiError::internal(other),
        }
    }
}

impl From<zcie_memory::Error> for ApiError {
    fn from(error: zcie_memory::Error) -> Self {
        match error {
            zcie_memory::Error::NotFound(m) => ApiError::NotFound(m),
            zcie_memory::Error::Invalid(m) => ApiError::InvalidRequest(m),
            other => ApiError::internal(other),
        }
    }
}

impl From<zcie_security::Error> for ApiError {
    fn from(error: zcie_security::Error) -> Self {
        match error {
            zcie_security::Error::Unauthorized => ApiError::Unauthorized,
            zcie_security::Error::Forbidden => ApiError::Forbidden("permission".into()),
            other => ApiError::internal(other),
        }
    }
}

impl From<zcie_store::Error> for ApiError {
    fn from(error: zcie_store::Error) -> Self {
        ApiError::internal(error)
    }
}

impl From<zcie_graph::Error> for ApiError {
    fn from(error: zcie_graph::Error) -> Self {
        ApiError::internal(error)
    }
}

/// Result type of the core API.
pub type ApiResult<T> = std::result::Result<T, ApiError>;
