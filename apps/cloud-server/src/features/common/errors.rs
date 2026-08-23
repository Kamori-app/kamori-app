//! HTTP/API error type and error response helpers.

use axum::http::StatusCode;
use serde::{Deserialize, Serialize};

use super::msgpack::MsgPack;

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
pub type ApiError = (StatusCode, MsgPack<ErrorResponse>);

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
        MsgPack(ErrorResponse::new("invalid_request", message)),
    )
}

/// Builds a 401 response with a message.
pub fn unauthenticated(message: &str) -> ApiError {
    (
        StatusCode::UNAUTHORIZED,
        MsgPack(ErrorResponse::new("unauthenticated", message)),
    )
}

/// Builds a 403 response with a message.
pub fn unauthorized(message: &str) -> ApiError {
    (
        StatusCode::FORBIDDEN,
        MsgPack(ErrorResponse::new("forbidden", message)),
    )
}

/// Builds a 409 response with a message.
pub fn conflict(message: &str) -> ApiError {
    (
        StatusCode::CONFLICT,
        MsgPack(ErrorResponse::new("conflict", message)),
    )
}

/// Builds a 429 response for a strict storage or egress quota.
pub fn quota_exceeded(message: &str) -> ApiError {
    (
        StatusCode::TOO_MANY_REQUESTS,
        MsgPack(ErrorResponse::new("quota_exceeded", message)),
    )
}

/// Builds a 404 response without revealing resources outside authorization.
pub fn not_found(message: &str) -> ApiError {
    (
        StatusCode::NOT_FOUND,
        MsgPack(ErrorResponse::new("not_found", message)),
    )
}

/// Builds a 500 response from an error.
pub fn internal_error<E: std::fmt::Display>(err: E) -> ApiError {
    let response = ErrorResponse::new("internal_error", "Unexpected server error");
    tracing::error!(request_id = %response.request_id, error = %err, "request failed");
    (StatusCode::INTERNAL_SERVER_ERROR, MsgPack(response))
}

#[cfg(test)]
mod tests {
    use axum::{body::to_bytes, response::IntoResponse};

    use super::*;

    #[tokio::test]
    async fn api_errors_use_the_messagepack_contract() {
        let response = bad_request("invalid input").into_response();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert_eq!(
            response
                .headers()
                .get(axum::http::header::CONTENT_TYPE)
                .and_then(|value| value.to_str().ok()),
            Some("application/msgpack")
        );
        let body = to_bytes(response.into_body(), 16 * 1024)
            .await
            .expect("read response");
        let error: ErrorResponse = rmp_serde::from_slice(&body).expect("decode MessagePack");
        assert_eq!(error.code, "invalid_request");
        assert_eq!(error.message, "invalid input");
    }
}
