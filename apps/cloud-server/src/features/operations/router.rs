//! Operation-log routes.

use axum::{Router, extract::DefaultBodyLimit, routing::post};

use crate::platform::state::AppState;

use super::handlers;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/operations", post(handlers::append).get(handlers::list))
        .layer(DefaultBodyLimit::max(26 * 1024 * 1024))
}
