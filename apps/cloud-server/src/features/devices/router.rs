//! Device routes.

use axum::{
    Router,
    routing::{delete, post},
};

use crate::platform::state::AppState;

use super::handlers;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/devices", post(handlers::register).get(handlers::list))
        .route("/devices/{device_id}", delete(handlers::revoke))
}
