//! Admission control for authenticated endpoints with large request bodies.

use axum::{
    extract::{Request, State},
    http::{Method, StatusCode, header},
    middleware::Next,
    response::{IntoResponse, Response},
};

use crate::{
    features::common::{ErrorResponse, MsgPack, authorize_session},
    platform::state::AppState,
};

const BYTE_PERMIT_SIZE: u64 = 64 * 1024;

fn is_large_protected_request(method: &Method, path: &str) -> bool {
    if method != Method::POST {
        return false;
    }
    path == "/operations"
        || (path.starts_with("/spaces/") && path.ends_with("/blobs"))
        || (path.starts_with("/spaces/")
            && (path.ends_with("/rotate-key") || path.ends_with("/revoke")))
}

fn rejection(status: StatusCode, code: &'static str, message: &'static str) -> Response {
    (status, MsgPack(ErrorResponse::new(code, message))).into_response()
}

pub async fn enforce(State(state): State<AppState>, request: Request, next: Next) -> Response {
    if !is_large_protected_request(request.method(), request.uri().path()) {
        return next.run(request).await;
    }
    if let Err(error) = authorize_session(&state, request.headers()).await {
        return error.into_response();
    }
    let Some(content_length) = request
        .headers()
        .get(header::CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
    else {
        return rejection(
            StatusCode::LENGTH_REQUIRED,
            "length_required",
            "Content-Length is required for this endpoint",
        );
    };
    if content_length > state.config.max_inflight_request_bytes {
        return rejection(
            StatusCode::PAYLOAD_TOO_LARGE,
            "payload_too_large",
            "request exceeds the process body budget",
        );
    }
    let Ok(_request_permit) = state.large_request_semaphore.clone().try_acquire_owned() else {
        return rejection(
            StatusCode::SERVICE_UNAVAILABLE,
            "server_busy",
            "large request concurrency limit reached",
        );
    };
    let permits = content_length.div_ceil(BYTE_PERMIT_SIZE).max(1);
    let Ok(permits) = u32::try_from(permits) else {
        return rejection(
            StatusCode::PAYLOAD_TOO_LARGE,
            "payload_too_large",
            "request is too large",
        );
    };
    let Ok(_byte_permits) = state
        .request_byte_semaphore
        .clone()
        .try_acquire_many_owned(permits)
    else {
        return rejection(
            StatusCode::SERVICE_UNAVAILABLE,
            "server_busy",
            "request body memory budget is exhausted",
        );
    };
    next.run(request).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_large_authenticated_writes_are_guarded() {
        assert!(is_large_protected_request(&Method::POST, "/operations"));
        assert!(is_large_protected_request(
            &Method::POST,
            "/spaces/00000000-0000-0000-0000-000000000001/blobs"
        ));
        assert!(is_large_protected_request(
            &Method::POST,
            "/spaces/00000000-0000-0000-0000-000000000001/rotate-key"
        ));
        assert!(is_large_protected_request(
            &Method::POST,
            "/spaces/00000000-0000-0000-0000-000000000001/members/00000000-0000-0000-0000-000000000002/revoke"
        ));
        assert!(!is_large_protected_request(&Method::GET, "/operations"));
        assert!(!is_large_protected_request(
            &Method::POST,
            "/auth/signin/start"
        ));
        assert!(!is_large_protected_request(
            &Method::POST,
            "/spaces/00000000-0000-0000-0000-000000000001/blobs/not-a-route"
        ));
    }
}
