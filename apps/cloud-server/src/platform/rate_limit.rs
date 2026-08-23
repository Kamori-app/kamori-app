//! Distributed request rate limiting backed by the shared Valkey state store.

use std::{
    net::{IpAddr, SocketAddr},
    time::Duration,
};

use axum::{
    extract::{ConnectInfo, Request, State},
    http::{HeaderValue, Method, StatusCode, header::RETRY_AFTER},
    middleware::Next,
    response::{IntoResponse, Response},
};
use sha2::{Digest, Sha256};

use crate::{
    features::common::{ApiError, ErrorResponse, MsgPack},
    platform::state::AppState,
};

pub(crate) const VERIFIED_CLIENT_IP_HEADER: &str = "x-kamori-verified-client-ip";

fn is_trusted_proxy(ip: IpAddr, trusted_proxy_cidrs: &[ipnet::IpNet]) -> bool {
    trusted_proxy_cidrs
        .iter()
        .any(|network| network.contains(&ip))
}

fn resolve_client_ip(request: &Request, trusted_proxy_cidrs: &[ipnet::IpNet]) -> Option<IpAddr> {
    let peer = request
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|connect| connect.0.ip())?;
    if !is_trusted_proxy(peer, trusted_proxy_cidrs) {
        return Some(peer);
    }

    let mut forwarded = request
        .headers()
        .get("x-forwarded-for")
        .and_then(|value| value.to_str().ok())
        .into_iter()
        .flat_map(|value| value.split(','))
        .filter_map(|value| value.trim().parse::<IpAddr>().ok())
        .collect::<Vec<_>>();
    if forwarded.is_empty()
        && let Some(real_ip) = request
            .headers()
            .get("x-real-ip")
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.trim().parse::<IpAddr>().ok())
    {
        forwarded.push(real_ip);
    }
    forwarded.push(peer);

    // Walk from the application back toward the client. Trusted proxy hops are
    // discarded; the first untrusted address is the effective client. This is
    // immune to client-supplied entries prepended to X-Forwarded-For.
    forwarded
        .into_iter()
        .rev()
        .find(|ip| !is_trusted_proxy(*ip, trusted_proxy_cidrs))
        .or(Some(peer))
}

fn source_identifier(client_ip: Option<IpAddr>) -> String {
    let source = client_ip
        .map(|ip| ip.to_string())
        .unwrap_or_else(|| "unknown".to_string());
    hex::encode(Sha256::digest(source.as_bytes()))
}

fn credential_identifier(scope: &str, identifier: &[u8]) -> String {
    let mut digest = Sha256::new();
    digest.update(b"kamori.auth-credential-rate-limit.v1\0");
    digest.update(scope.as_bytes());
    digest.update([0]);
    digest.update(identifier);
    hex::encode(digest.finalize())
}

/// Applies a second, IP-independent limit to one normalized account or
/// credential. Raw identifiers never leave process memory or enter Valkey.
pub async fn enforce_credential_attempt(
    state: &AppState,
    scope: &str,
    identifier: &[u8],
) -> Result<(), ApiError> {
    let key = format!(
        "rate:credential:{}",
        credential_identifier(scope, identifier)
    );
    let count = state
        .state_store
        .increment(&key, Duration::from_secs(60))
        .await
        .map_err(|error| {
            tracing::error!(%error, "credential rate-limit backend unavailable");
            (
                StatusCode::SERVICE_UNAVAILABLE,
                MsgPack(ErrorResponse::new(
                    "auth_guard_unavailable",
                    "authentication guard unavailable",
                )),
            )
        })?;
    if count > state.config.auth_rate_limit_per_minute {
        return Err((
            StatusCode::TOO_MANY_REQUESTS,
            MsgPack(ErrorResponse::new(
                "rate_limited",
                "authentication attempt limit exceeded",
            )),
        ));
    }
    Ok(())
}

fn is_credential_endpoint(path: &str) -> bool {
    [
        "/auth/signup/",
        "/auth/signin/",
        "/auth/account-recovery/",
        "/auth/passkey/login/",
        "/auth/device-authorization/",
        "/auth/reauth/",
        "/auth/refresh",
        "/admin-api/bootstrap/",
        "/admin-api/auth/",
    ]
    .iter()
    .any(|prefix| path.starts_with(prefix))
}

fn error_response(status: StatusCode, message: &str) -> Response {
    (status, MsgPack(ErrorResponse::new("rate_limited", message))).into_response()
}

fn authenticated_session_identifier(state: &AppState, request: &Request) -> Option<String> {
    let token = crate::platform::security::auth::bearer_from_headers(request.headers())?;
    let claims = state.validate_token(&token).ok()?;
    if claims.kind != crate::platform::jwt::TokenKind::Session {
        return None;
    }
    let session_id = claims.session_id?;
    let mut digest = Sha256::new();
    digest.update(b"kamori.session-rate-limit.v1\0");
    digest.update(claims.user_id.as_bytes());
    digest.update(session_id.as_bytes());
    Some(hex::encode(digest.finalize()))
}

fn request_cost(method: &Method, path: &str) -> u64 {
    if path.contains("/blobs") {
        20
    } else if path.ends_with("/rotate-key") || path.ends_with("/revoke") {
        25
    } else if path == "/operations" && method == Method::POST {
        4
    } else if path == "/operations" {
        2
    } else {
        1
    }
}

pub async fn enforce(State(state): State<AppState>, mut request: Request, next: Next) -> Response {
    if request.method() == Method::OPTIONS {
        return next.run(request).await;
    }
    if request.uri().path() == "/health/live" {
        return next.run(request).await;
    }
    let client_ip = resolve_client_ip(&request, &state.config.trusted_proxy_cidrs);
    request.headers_mut().remove(VERIFIED_CLIENT_IP_HEADER);
    if let Some(client_ip) = client_ip
        && let Ok(value) = HeaderValue::from_str(&client_ip.to_string())
    {
        request
            .headers_mut()
            .insert(VERIFIED_CLIENT_IP_HEADER, value);
    }
    let source = source_identifier(client_ip);
    let path = request.uri().path().to_owned();
    let credential_endpoint = is_credential_endpoint(&path);
    let ttl = Duration::from_secs(60);
    let global_key = format!("rate:api:{source}");
    let global_count = match state.state_store.increment(&global_key, ttl).await {
        Ok(count) => count,
        Err(error) => {
            tracing::error!(%error, "rate-limit backend unavailable");
            if credential_endpoint {
                return error_response(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "authentication guard unavailable",
                );
            }
            // Auth remains fail-closed because an outage must not remove the
            // online guessing limit. Authenticated encrypted sync remains
            // available: PostgreSQL authorization, signatures, operation size
            // limits and blob quota admission still apply independently.
            return next.run(request).await;
        }
    };
    let limited = if global_count > state.config.api_rate_limit_per_minute {
        true
    } else if credential_endpoint {
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
    } else if let Some(session) = authenticated_session_identifier(&state, &request) {
        let key = format!("rate:session:{session}");
        match state
            .state_store
            .increment_by(&key, request_cost(request.method(), &path), ttl)
            .await
        {
            Ok(units) => units > state.config.session_rate_limit_units_per_minute,
            Err(error) => {
                tracing::error!(%error, "session rate-limit backend unavailable");
                false
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
    use axum::body::Body;

    fn request(peer: &str, forwarded: Option<&str>) -> Request {
        let mut builder = Request::builder().uri("/operations");
        if let Some(forwarded) = forwarded {
            builder = builder.header("x-forwarded-for", forwarded);
        }
        let mut request = builder.body(Body::empty()).expect("request");
        request.extensions_mut().insert(ConnectInfo(
            peer.parse::<SocketAddr>().expect("peer address"),
        ));
        request
    }

    #[test]
    fn credential_paths_are_scoped_strictly() {
        assert!(is_credential_endpoint("/auth/signin/start"));
        assert!(is_credential_endpoint("/auth/refresh"));
        assert!(is_credential_endpoint("/auth/device-authorization/start"));
        assert!(is_credential_endpoint("/auth/reauth/start"));
        assert!(is_credential_endpoint("/admin-api/auth/start"));
        assert!(!is_credential_endpoint("/auth/sessions"));
        assert!(!is_credential_endpoint("/operations"));
    }

    #[test]
    fn credential_keys_are_scoped_and_do_not_contain_user_input() {
        let first = credential_identifier("signin", b"alice");
        assert_eq!(first.len(), 64);
        assert!(!first.contains("alice"));
        assert_ne!(first, credential_identifier("recovery", b"alice"));
        assert_ne!(first, credential_identifier("signin", b"bob"));
    }

    #[test]
    fn expensive_endpoints_consume_more_session_units() {
        assert_eq!(request_cost(&Method::GET, "/workspaces"), 1);
        assert_eq!(request_cost(&Method::GET, "/operations"), 2);
        assert_eq!(request_cost(&Method::POST, "/operations"), 4);
        assert_eq!(
            request_cost(
                &Method::POST,
                "/spaces/00000000-0000-0000-0000-000000000001/blobs"
            ),
            20
        );
        assert_eq!(
            request_cost(
                &Method::POST,
                "/spaces/00000000-0000-0000-0000-000000000001/rotate-key"
            ),
            25
        );
    }

    #[test]
    fn untrusted_peer_cannot_spoof_forwarded_address() {
        let request = request("203.0.113.7:443", Some("198.51.100.99"));
        assert_eq!(
            resolve_client_ip(&request, &[]),
            Some("203.0.113.7".parse().unwrap())
        );
    }

    #[test]
    fn trusted_proxy_chain_uses_nearest_untrusted_hop() {
        let request = request("172.30.0.2:443", Some("198.51.100.99, 203.0.113.7"));
        let trusted = vec!["172.30.0.2/32".parse().expect("trusted proxy")];
        assert_eq!(
            resolve_client_ip(&request, &trusted),
            Some("203.0.113.7".parse().unwrap())
        );
    }
}
