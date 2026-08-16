//! Top-level HTTP router composition and CORS policy setup.

use axum::{
    Router,
    http::{HeaderName, HeaderValue, Method},
    middleware,
};
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;

use crate::{platform::config::Config, platform::state::AppState};

fn parse_origin_values(values: &[String]) -> anyhow::Result<Vec<HeaderValue>> {
    values
        .iter()
        .map(|value| {
            HeaderValue::from_str(value)
                .map_err(|error| anyhow::anyhow!("invalid CORS origin {value:?}: {error}"))
        })
        .collect()
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

fn build_cors_layer(config: &Config) -> anyhow::Result<CorsLayer> {
    let mut cors = CorsLayer::new();
    let origins_allow_any = config.cors_allow_origins.iter().any(|value| value == "*");

    if config.cors_allow_credentials && origins_allow_any {
        anyhow::bail!(
            "KAMORI_CORS_ALLOW_CREDENTIALS=true requires explicit KAMORI_CORS_ALLOW_ORIGINS (not `*`)"
        );
    }

    if origins_allow_any {
        cors = cors.allow_origin(Any);
    } else {
        cors = cors.allow_origin(parse_origin_values(&config.cors_allow_origins)?);
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

    Ok(cors)
}

pub fn build_router(state: AppState) -> anyhow::Result<Router> {
    let cors_layer = build_cors_layer(&state.config)?;
    let rate_limit_state = state.clone();
    let http_metrics = state.http_metrics.clone();

    Ok(Router::new()
        .merge(crate::features::health::router::router())
        .merge(crate::features::admin::router::router())
        .merge(crate::features::auth::router::router())
        .merge(crate::features::workspaces::router::router())
        .merge(crate::features::users::router::router())
        .merge(crate::features::devices::router::router())
        .merge(crate::features::cas::router::router())
        .merge(crate::features::spaces::router::router())
        .merge(crate::features::operations::router::router())
        .merge(crate::features::ownership::router::router())
        .merge(crate::features::invites::router::router())
        .layer(middleware::from_fn_with_state(
            http_metrics,
            crate::platform::metrics::record_http,
        ))
        .layer(middleware::from_fn_with_state(
            rate_limit_state,
            crate::platform::rate_limit::enforce,
        ))
        .layer(cors_layer)
        .layer(TraceLayer::new_for_http())
        .with_state(state))
}
