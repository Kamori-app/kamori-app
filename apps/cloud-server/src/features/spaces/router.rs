//! Security-space routes.

use axum::{
    Router,
    routing::{delete, get, post},
};

use crate::platform::state::AppState;

use super::handlers;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/spaces", post(handlers::create).get(handlers::list))
        .route("/spaces/trash", get(handlers::list_trash))
        .route("/spaces/{space_id}", delete(handlers::move_to_trash))
        .route(
            "/spaces/{space_id}/restore",
            post(handlers::restore_from_trash),
        )
        .route("/spaces/{space_id}/members", get(handlers::list_members))
        .route("/spaces/{space_id}/devices", get(handlers::list_devices))
        .route(
            "/spaces/{space_id}/device-key-packages",
            post(handlers::put_device_key_package),
        )
        .route(
            "/spaces/{space_id}/recovery-key-package",
            post(handlers::put_recovery_key_package),
        )
        .route(
            "/spaces/{space_id}/members/{user_id}/revoke",
            post(handlers::revoke_member),
        )
}
