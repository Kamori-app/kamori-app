//! Routes exposed only to the separately deployed operator frontend.

use axum::{
    Router,
    routing::{get, post},
};

use crate::platform::state::AppState;

use super::handlers;

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/admin-api/bootstrap/start",
            post(handlers::bootstrap_start),
        )
        .route(
            "/admin-api/bootstrap/finish",
            post(handlers::bootstrap_finish),
        )
        .route("/admin-api/auth/start", post(handlers::login_start))
        .route("/admin-api/auth/finish", post(handlers::login_finish))
        .route("/admin-api/auth/reauth/start", post(handlers::reauth_start))
        .route(
            "/admin-api/auth/reauth/finish",
            post(handlers::reauth_finish),
        )
        .route("/admin-api/auth/logout", post(handlers::logout))
        .route(
            "/admin-api/security-keys/add/start",
            post(handlers::add_security_key_start),
        )
        .route(
            "/admin-api/security-keys/add/finish",
            post(handlers::add_security_key_finish),
        )
        .route(
            "/admin-api/security-keys/remove",
            post(handlers::remove_security_key),
        )
        .route("/admin-api/dashboard", get(handlers::dashboard))
        .route(
            "/admin-api/settings",
            get(handlers::settings).post(handlers::update_setting),
        )
        .route("/admin-api/accounts/suspension", post(handlers::suspend))
        .route("/admin-api/audit", get(handlers::audit))
}
