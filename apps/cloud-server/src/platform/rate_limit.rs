//! Distributed request rate limiting backed by the shared Valkey state store.

use std::{net::SocketAddr, time::Duration};

use axum::{
    Json,
    extract::{ConnectInfo, Request, State},
    http::{HeaderValue, Method, StatusCode, header::RETRY_AFTER},
    middleware::Next,
    response::{IntoResponse, Response},
};
use sha2::{Digest, Sha256};

use crate::{features::common::ErrorResponse, platform::state::AppState};

fn source_identifier(request: &Request) -> String {
    let forwarded = request
        .headers()
        .get("x-forwarded-for")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(',').next())
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let source = forwarded
        .map(str::to_owned)
        .or_else(|| {
            request
                .extensions()
                .get::<ConnectInfo<SocketAddr>>()
                .map(|connect| connect.0.ip().to_string())
        })
        .unwrap_or_else(|| "unknown".to_string());
    hex::encode(Sha256::digest(source.as_bytes()))
}

fn is_credential_endpoint(path: &str) -> bool {
    [
        "/auth/signup/",
        "/auth/signin/",
        "/auth/account-recovery/",
        "/auth/passkey/login/",
        "/auth/refresh",
        "/admin-api/bootstrap/",
        "/admin-api/auth/",
    ]
    .iter()
    .any(|prefix| path.starts_with(prefix))
}

fn error_response(status: StatusCode, message: &str) -> Response {
    (status, Json(ErrorResponse::new("rate_limited", message))).into_response()
}

pub async fn enforce(State(state): State<AppState>, request: Request, next: Next) -> Response {
    if request.method() == Method::OPTIONS {
        return next.run(request).await;
    }
    if request.uri().path() == "/health/live" {
        return next.run(request).await;
    }
    let source = source_identifier(&request);
    let path = request.uri().path().to_owned();
    let ttl = Duration::from_secs(60);
    let global_key = format!("rate:api:{source}");
    let global_count = match state.state_store.increment(&global_key, ttl).await {
        Ok(count) => count,
        Err(error) => {
            tracing::error!(%error, "rate-limit backend unavailable");
            return error_response(StatusCode::SERVICE_UNAVAILABLE, "request guard unavailable");
        }
    };
    let limited = if global_count > state.config.api_rate_limit_per_minute {
        true
    } else if is_credential_endpoint(&path) {
        let auth_key = format!("rate:auth:{source}");
        match state.state_store.increment(&auth_key, ttl).await {
            Ok(count) => count > state.config.auth_rate_limit_per_minute,
            Err(error) => {
                tracing::error!(%error, "authentication rate-limit backend unavailable");
                return error_response(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "authentication guard unavailable",
                );
            }
        }
    } else {
        false
    };
    if limited {
        let mut response = error_response(StatusCode::TOO_MANY_REQUESTS, "rate limit exceeded");
        response
            .headers_mut()
            .insert(RETRY_AFTER, HeaderValue::from_static("60"));
        return response;
    }
    next.run(request).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn credential_paths_are_scoped_strictly() {
        assert!(is_credential_endpoint("/auth/signin/start"));
        assert!(is_credential_endpoint("/auth/refresh"));
        assert!(is_credential_endpoint("/admin-api/auth/start"));
        assert!(!is_credential_endpoint("/auth/sessions"));
        assert!(!is_credential_endpoint("/operations"));
    }
}
