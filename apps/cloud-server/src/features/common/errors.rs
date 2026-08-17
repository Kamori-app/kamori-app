//! HTTP/API error type and error response helpers.

use axum::{Json, http::StatusCode};
use serde::{Deserialize, Serialize};

/// Error response payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorResponse {
    /// Stable machine-readable error code.
    pub code: String,
    /// Safe user-facing error message.
    pub message: String,
    /// Correlation id for server-side diagnostics.
    pub request_id: String,
}

/// Common API error shape used by handlers.
pub type ApiError = (StatusCode, Json<ErrorResponse>);

impl ErrorResponse {
    pub(crate) fn new(code: &str, message: &str) -> Self {
        Self {
            code: code.to_owned(),
            message: message.to_owned(),
            request_id: uuid::Uuid::new_v4().to_string(),
        }
    }
}

/// Builds a 400 response with a message.
pub fn bad_request(message: &str) -> ApiError {
    (
        StatusCode::BAD_REQUEST,
        Json(ErrorResponse::new("invalid_request", message)),
    )
}

/// Builds a 401 response with a message.
pub fn unauthenticated(message: &str) -> ApiError {
    (
        StatusCode::UNAUTHORIZED,
        Json(ErrorResponse::new("unauthenticated", message)),
    )
}

/// Builds a 403 response with a message.
pub fn unauthorized(message: &str) -> ApiError {
    (
        StatusCode::FORBIDDEN,
        Json(ErrorResponse::new("forbidden", message)),
    )
}

/// Builds a 409 response with a message.
pub fn conflict(message: &str) -> ApiError {
    (
        StatusCode::CONFLICT,
        Json(ErrorResponse::new("conflict", message)),
    )
}

/// Builds a 429 response for a strict storage or egress quota.
pub fn quota_exceeded(message: &str) -> ApiError {
    (
        StatusCode::TOO_MANY_REQUESTS,
        Json(ErrorResponse::new("quota_exceeded", message)),
    )
}

/// Builds a 404 response without revealing resources outside authorization.
pub fn not_found(message: &str) -> ApiError {
    (
        StatusCode::NOT_FOUND,
        Json(ErrorResponse::new("not_found", message)),
    )
}

/// Builds a 500 response from an error.
pub fn internal_error<E: std::fmt::Display>(err: E) -> ApiError {
    let response = ErrorResponse::new("internal_error", "Unexpected server error");
    tracing::error!(request_id = %response.request_id, error = %err, "request failed");
    (StatusCode::INTERNAL_SERVER_ERROR, Json(response))
}
