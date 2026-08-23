//! Device routes.

use axum::{Router, routing::post};

use crate::platform::state::AppState;

use super::handlers;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/devices", post(handlers::register).get(handlers::list))
        .route("/devices/{device_id}/revoke", post(handlers::revoke))
}
