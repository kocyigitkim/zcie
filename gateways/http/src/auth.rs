//! Authentication and Origin check (`docs/11-security.md` sections 2 and 3).

use axum::http::HeaderMap;
use axum::http::header::{AUTHORIZATION, ORIGIN};
use zcie_api::{ApiError, Engine};
use zcie_security::AuthContext;

/// Returns an error if the request has an `Origin` that is not a loopback origin.
pub fn check_origin(headers: &HeaderMap) -> Result<(), ApiError> {
    let Some(origin) = headers.get(ORIGIN) else { return Ok(()) };
    let origin = origin.to_str().unwrap_or_default();
    let host = origin.strip_prefix("http://").unwrap_or("");
    let host = host.split(':').next().unwrap_or_default();
    if host == "127.0.0.1" || host == "localhost" || host == "[::1]" {
        Ok(())
    } else {
        Err(ApiError::Forbidden("origin is not permitted".into()))
    }
}

/// Checks the Origin and the bearer key. Returns the auth context.
pub fn authenticate(engine: &Engine, headers: &HeaderMap) -> Result<AuthContext, ApiError> {
    check_origin(headers)?;
    let header = headers.get(AUTHORIZATION).and_then(|v| v.to_str().ok());
    engine.authenticate(header)
}

/// Like `authenticate`, but a request with no `Authorization` header gives `None`.
pub fn authenticate_optional(engine: &Engine, headers: &HeaderMap) -> Result<Option<AuthContext>, ApiError> {
    check_origin(headers)?;
    if headers.get(AUTHORIZATION).is_none() {
        return Ok(None);
    }
    authenticate(engine, headers).map(Some)
}
