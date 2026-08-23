//! Top-level HTTP router composition and CORS policy setup.

use axum::{
    Router,
    http::{HeaderName, HeaderValue, Method},
    middleware,
};
use std::time::Duration;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;
use url::{Host, Url};

use crate::{platform::config::Config, platform::state::AppState};

fn parse_origin_values(values: &[String]) -> anyhow::Result<Vec<HeaderValue>> {
    values
        .iter()
        .map(|value| {
            validate_origin(value)?;
            HeaderValue::from_str(value)
                .map_err(|error| anyhow::anyhow!("invalid CORS origin {value:?}: {error}"))
        })
        .collect()
}

fn validate_origin(value: &str) -> anyhow::Result<()> {
    // Tauri's development origin is not a special URL origin according to the
    // generic URL parser, so accept only its exact, path-free spelling.
    if value == "tauri://localhost" {
        return Ok(());
    }
    let url = Url::parse(value)
        .map_err(|error| anyhow::anyhow!("invalid CORS origin {value:?}: {error}"))?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.path() != "/"
        || url.query().is_some()
        || url.fragment().is_some()
    {
        anyhow::bail!("invalid CORS origin {value:?}: expected a path-free http(s) origin");
    }
    if url.scheme() == "http" {
        let loopback = match url.host() {
            Some(Host::Domain(host)) => host.eq_ignore_ascii_case("localhost"),
            Some(Host::Ipv4(address)) => address.is_loopback(),
            Some(Host::Ipv6(address)) => address.is_loopback(),
            None => false,
        };
        if !loopback {
            anyhow::bail!("insecure non-loopback CORS origin {value:?} is not allowed");
        }
    }
    if url.origin().ascii_serialization() != value {
        anyhow::bail!(
            "invalid CORS origin {value:?}: use the canonical origin without a trailing slash"
        );
    }
    Ok(())
}

fn validate_cookie_origin(value: &str) -> anyhow::Result<()> {
    validate_origin(value)?;
    let url = Url::parse(value)
        .map_err(|error| anyhow::anyhow!("invalid browser cookie origin {value:?}: {error}"))?;
    if !matches!(url.scheme(), "http" | "https") {
        anyhow::bail!("browser cookie origins must use http or https");
    }
    Ok(())
}

fn parse_method_values(values: &[String]) -> anyhow::Result<Vec<Method>> {
    values
        .iter()
        .map(|value| {
            value
                .parse::<Method>()
                .map_err(|error| anyhow::anyhow!("invalid CORS method {value:?}: {error}"))
        })
        .collect()
}

fn parse_header_values(values: &[String]) -> anyhow::Result<Vec<HeaderName>> {
    values
        .iter()
        .map(|value| {
            value
                .parse::<HeaderName>()
                .map_err(|error| anyhow::anyhow!("invalid CORS header {value:?}: {error}"))
        })
        .collect()
}

fn build_cors_layer(config: &Config, allowed_origins: &[String]) -> anyhow::Result<CorsLayer> {
    let mut cors = CorsLayer::new();
    let origins_allow_any = allowed_origins.iter().any(|value| value == "*");

    if origins_allow_any && allowed_origins.len() != 1 {
        anyhow::bail!("a CORS policy cannot mix `*` with explicit origins");
    }

    if config.cors_allow_credentials && origins_allow_any {
        anyhow::bail!(
            "KAMORI_CORS_ALLOW_CREDENTIALS=true requires explicit KAMORI_CORS_ALLOW_ORIGINS (not `*`)"
        );
    }

    if origins_allow_any {
        cors = cors.allow_origin(Any);
    } else {
        cors = cors.allow_origin(parse_origin_values(allowed_origins)?);
    }

    if config.cors_allow_methods.iter().any(|value| value == "*") {
        cors = cors.allow_methods(Any);
    } else {
        cors = cors.allow_methods(parse_method_values(&config.cors_allow_methods)?);
    }

    if config.cors_allow_headers.iter().any(|value| value == "*") {
        cors = cors.allow_headers(Any);
    } else {
        cors = cors.allow_headers(parse_header_values(&config.cors_allow_headers)?);
    }

    if config.cors_allow_credentials {
        cors = cors.allow_credentials(true);
    }

    Ok(cors.max_age(Duration::from_secs(60 * 60)))
}

pub fn build_router(state: AppState) -> anyhow::Result<Router> {
    if state.config.web_cookie_origins.is_empty() {
        anyhow::bail!("KAMORI_WEB_COOKIE_ORIGINS must contain at least one exact browser origin");
    }
    for origin in &state.config.web_cookie_origins {
        if origin == "*" {
            anyhow::bail!("KAMORI_WEB_COOKIE_ORIGINS does not support a wildcard");
        }
        validate_cookie_origin(origin)?;
    }
    let consumer_cors = build_cors_layer(&state.config, &state.config.cors_allow_origins)?;
    let admin_cors = build_cors_layer(&state.config, &state.config.admin_cors_allow_origins)?;
    let rate_limit_state = state.clone();
    let request_guard_state = state.clone();
    let http_metrics = state.http_metrics.clone();

    let consumer_router = Router::new()
        .merge(crate::features::health::router::router())
        .merge(crate::features::auth::router::router())
        .merge(crate::features::workspaces::router::router())
        .merge(crate::features::users::router::router())
        .merge(crate::features::devices::router::router())
        .merge(crate::features::cas::router::router())
        .merge(crate::features::spaces::router::router())
        .merge(crate::features::operations::router::router())
        .merge(crate::features::ownership::router::router())
        .merge(crate::features::invites::router::router())
        .layer(consumer_cors);
    let admin_router = crate::features::admin::router::router().layer(admin_cors);

    Ok(Router::new()
        .merge(consumer_router)
        .merge(admin_router)
        .layer(middleware::from_fn_with_state(
            request_guard_state,
            crate::platform::request_guard::enforce,
        ))
        .layer(middleware::from_fn_with_state(
            http_metrics,
            crate::platform::metrics::record_http,
        ))
        .layer(middleware::from_fn_with_state(
            rate_limit_state,
            crate::platform::rate_limit::enforce,
        ))
        .layer(TraceLayer::new_for_http())
        .with_state(state))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, http::Request, routing::post};
    use tower::ServiceExt;

    async fn endpoint() {}

    async fn preflight(origin: &str) -> axum::response::Response {
        let mut config = crate::platform::test_support::test_config();
        config.cors_allow_origins = vec![
            "https://kamori.app".to_string(),
            "https://app.kamori.app".to_string(),
        ];
        config.cors_allow_methods = vec!["POST".to_string(), "OPTIONS".to_string()];
        config.cors_allow_headers = vec!["content-type".to_string(), "accept".to_string()];

        Router::new()
            .route("/auth/signup/start", post(endpoint))
            .layer(build_cors_layer(&config, &config.cors_allow_origins).expect("CORS layer"))
            .oneshot(
                Request::builder()
                    .method(Method::OPTIONS)
                    .uri("/auth/signup/start")
                    .header("origin", origin)
                    .header("access-control-request-method", "POST")
                    .header("access-control-request-headers", "content-type,accept")
                    .body(Body::empty())
                    .expect("preflight request"),
            )
            .await
            .expect("preflight response")
    }

    #[tokio::test]
    async fn signup_preflight_allows_canonical_web_origins() {
        for origin in ["https://kamori.app", "https://app.kamori.app"] {
            let response = preflight(origin).await;
            assert_eq!(response.status(), axum::http::StatusCode::OK);
            assert_eq!(
                response
                    .headers()
                    .get("access-control-allow-origin")
                    .and_then(|value| value.to_str().ok()),
                Some(origin)
            );
            assert_eq!(
                response
                    .headers()
                    .get("access-control-allow-credentials")
                    .and_then(|value| value.to_str().ok()),
                Some("true")
            );
        }
    }

    #[tokio::test]
    async fn signup_preflight_rejects_unlisted_origin() {
        let response = preflight("https://attacker.example").await;
        assert!(
            response
                .headers()
                .get("access-control-allow-origin")
                .is_none()
        );
    }

    #[test]
    fn cors_origins_reject_paths_credentials_and_remote_http() {
        for origin in [
            "https://kamori.app/path",
            "https://user@kamori.app",
            "http://kamori.app",
            "https://kamori.app/",
        ] {
            assert!(validate_origin(origin).is_err(), "accepted {origin}");
        }
        for origin in [
            "https://kamori.app",
            "http://localhost:4173",
            "http://127.0.0.1:4173",
            "tauri://localhost",
        ] {
            validate_origin(origin).unwrap_or_else(|error| panic!("rejected {origin}: {error}"));
        }
    }

    #[test]
    fn cors_origins_reject_mixed_wildcard() {
        let mut config = crate::platform::test_support::test_config();
        config.cors_allow_origins = vec!["*".to_string(), "https://kamori.app".to_string()];
        assert!(build_cors_layer(&config, &config.cors_allow_origins).is_err());
    }

    #[test]
    fn cookie_origins_reject_non_browser_schemes() {
        assert!(validate_cookie_origin("tauri://localhost").is_err());
        assert!(validate_cookie_origin("https://app.kamori.app").is_ok());
        assert!(validate_cookie_origin("http://localhost:4173").is_ok());
    }

    async fn scoped_preflight(path: &str, origin: &str) -> axum::response::Response {
        let mut config = crate::platform::test_support::test_config();
        config.cors_allow_origins = vec!["https://app.kamori.app".to_string()];
        config.admin_cors_allow_origins = vec!["https://admin.kamori.app".to_string()];
        config.cors_allow_methods = vec!["POST".to_string(), "OPTIONS".to_string()];

        Router::new()
            .merge(Router::new().route("/consumer", post(endpoint)).layer(
                build_cors_layer(&config, &config.cors_allow_origins).expect("consumer CORS"),
            ))
            .merge(Router::new().route("/admin", post(endpoint)).layer(
                build_cors_layer(&config, &config.admin_cors_allow_origins).expect("admin CORS"),
            ))
            .oneshot(
                Request::builder()
                    .method(Method::OPTIONS)
                    .uri(path)
                    .header("origin", origin)
                    .header("access-control-request-method", "POST")
                    .body(Body::empty())
                    .expect("preflight request"),
            )
            .await
            .expect("preflight response")
    }

    #[tokio::test]
    async fn consumer_and_admin_origins_are_not_interchangeable() {
        let consumer = scoped_preflight("/consumer", "https://app.kamori.app").await;
        assert_eq!(
            consumer.headers().get("access-control-allow-origin"),
            Some(&HeaderValue::from_static("https://app.kamori.app"))
        );
        let admin_on_consumer = scoped_preflight("/consumer", "https://admin.kamori.app").await;
        assert!(
            admin_on_consumer
                .headers()
                .get("access-control-allow-origin")
                .is_none()
        );

        let admin = scoped_preflight("/admin", "https://admin.kamori.app").await;
        assert_eq!(
            admin.headers().get("access-control-allow-origin"),
            Some(&HeaderValue::from_static("https://admin.kamori.app"))
        );
        let consumer_on_admin = scoped_preflight("/admin", "https://app.kamori.app").await;
        assert!(
            consumer_on_admin
                .headers()
                .get("access-control-allow-origin")
                .is_none()
        );
    }
}
