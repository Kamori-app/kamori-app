//! HTTP/API error type and error response helpers.

use axum::{Json, http::StatusCode};
use serde::{Deserialize, Serialize};

/// Error response payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorResponse {
    /// Error message.
    pub error: String,
}

/// Common API error shape used by handlers.
pub type ApiError = (StatusCode, Json<ErrorResponse>);

/// Builds a 400 response with a message.
pub fn bad_request(message: &str) -> ApiError {
    (
        StatusCode::BAD_REQUEST,
        Json(ErrorResponse {
            error: message.to_string(),
        }),
    )
}

/// Builds a 401 response with a message.
pub fn unauthenticated(message: &str) -> ApiError {
    (
        StatusCode::UNAUTHORIZED,
        Json(ErrorResponse {
            error: message.to_string(),
        }),
    )
}

/// Builds a 403 response with a message.
pub fn unauthorized(message: &str) -> ApiError {
    (
        StatusCode::FORBIDDEN,
        Json(ErrorResponse {
            error: message.to_string(),
        }),
    )
}

/// Builds a 409 response with a message.
pub fn conflict(message: &str) -> ApiError {
    (
        StatusCode::CONFLICT,
        Json(ErrorResponse {
            error: message.to_string(),
        }),
    )
}

/// Builds a 429 response for a strict storage or egress quota.
pub fn quota_exceeded(message: &str) -> ApiError {
    (
        StatusCode::TOO_MANY_REQUESTS,
        Json(ErrorResponse {
            error: message.to_string(),
        }),
    )
}

/// Builds a 404 response without revealing resources outside authorization.
pub fn not_found(message: &str) -> ApiError {
    (
        StatusCode::NOT_FOUND,
        Json(ErrorResponse {
            error: message.to_string(),
        }),
    )
}

/// Builds a 500 response from an error.
pub fn internal_error<E: std::fmt::Display>(err: E) -> ApiError {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(ErrorResponse {
            error: err.to_string(),
        }),
    )
}
