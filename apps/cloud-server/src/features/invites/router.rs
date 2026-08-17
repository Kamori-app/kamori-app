//! Router for invite-code endpoints.

use axum::{Router, routing::post};

use crate::platform::state::AppState;

use super::handlers;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/invite-codes", post(handlers::create_invite_code))
        .route("/invite-codes/redeem", post(handlers::redeem_invite_code))
}
