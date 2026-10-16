//! HTTP error responses (`docs/10-api-and-gateways.md` section 2.1).

use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use zcie_api::ApiError;
use zcie_types::api::{ErrorBody, ErrorDetail};

/// An `ApiError` as an HTTP response.
pub struct HttpError(pub ApiError);

impl From<ApiError> for HttpError {
    fn from(error: ApiError) -> Self {
        HttpError(error)
    }
}

impl IntoResponse for HttpError {
    fn into_response(self) -> Response {
        let status = StatusCode::from_u16(self.0.http_status()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
        let message = match &self.0 {
            ApiError::Forbidden(m) | ApiError::NotFound(m) | ApiError::InvalidRequest(m) | ApiError::Internal(m) => m.clone(),
            other => other.to_string(),
        };
        let body = ErrorBody { error: ErrorDetail { code: self.0.code().to_string(), message } };
        (status, Json(body)).into_response()
    }
}

/// Result type of route handlers.
pub type HttpResult<T> = std::result::Result<T, HttpError>;
