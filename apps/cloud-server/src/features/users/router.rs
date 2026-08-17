//! Router for user-account endpoints.

use axum::{
    Router,
    routing::{get, post},
};

use crate::platform::state::AppState;

use super::handlers;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/users/me/delete", post(handlers::delete_me))
        .route("/users/me/deletion-status", get(handlers::deletion_status))
        .route(
            "/users/me/consents",
            get(handlers::get_consents).post(handlers::update_consents),
        )
}
