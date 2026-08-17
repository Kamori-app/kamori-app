//! Router for healthcheck endpoint.

use axum::{Router, routing::get};

use crate::platform::state::AppState;

use super::handlers;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/health", get(handlers::ready))
        .route("/health/live", get(handlers::live))
        .route("/health/ready", get(handlers::ready))
        .route("/metrics", get(handlers::metrics))
}
