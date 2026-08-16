//! Ownership transfer routes.

use axum::{
    Router,
    routing::{delete, get, post},
};

use crate::platform::state::AppState;

use super::handlers;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/ownership-transfers", post(handlers::create))
        .route(
            "/ownership-transfers/incoming",
            get(handlers::list_incoming),
        )
        .route(
            "/ownership-transfers/outgoing",
            get(handlers::list_outgoing),
        )
        .route(
            "/ownership-transfers/{transfer_id}/accept",
            post(handlers::accept),
        )
        .route(
            "/ownership-transfers/{transfer_id}",
            delete(handlers::cancel),
        )
}
