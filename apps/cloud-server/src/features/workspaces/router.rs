//! Router for workspace primitive endpoints.

use axum::{Router, routing::post};

use crate::platform::state::AppState;

use super::handlers;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/workspaces", post(handlers::create_workspace))
        .route("/workspaces/list", post(handlers::list_workspaces))
        .route(
            "/workspaces/members",
            post(handlers::list_workspace_members),
        )
        .route(
            "/workspaces/members/role",
            post(handlers::update_workspace_member_role),
        )
        .route(
            "/workspaces/members/revoke",
            post(handlers::revoke_workspace_member),
        )
}
