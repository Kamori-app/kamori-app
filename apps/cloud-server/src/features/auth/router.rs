//! Router for auth-related endpoints.

use axum::{
    Router,
    routing::{get, post},
};

use crate::platform::state::AppState;

use super::handlers;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/auth/signup/start", post(handlers::signup_start))
        .route("/auth/signup/finish", post(handlers::signup_finish))
        .route(
            "/auth/password/change/start",
            post(handlers::password_change_start),
        )
        .route(
            "/auth/password/change/finish",
            post(handlers::password_change_finish),
        )
        .route(
            "/auth/account-recovery/start",
            post(handlers::account_recovery_start),
        )
        .route(
            "/auth/account-recovery/finish",
            post(handlers::account_recovery_finish),
        )
        .route("/auth/signin/start", post(handlers::signin_start))
        .route("/auth/signin/finish", post(handlers::signin_finish))
        .route("/auth/signin/totp", post(handlers::signin_totp))
        .route("/auth/reauth/start", post(handlers::reauth_start))
        .route("/auth/reauth/finish", post(handlers::reauth_finish))
        .route("/auth/refresh", post(handlers::refresh))
        .route("/auth/csrf", post(handlers::csrf_bootstrap))
        .route("/auth/logout", post(handlers::logout))
        .route(
            "/auth/device-authorization/start",
            post(handlers::device_authorization_start),
        )
        .route(
            "/auth/device-authorization/inspect",
            post(handlers::device_authorization_inspect),
        )
        .route(
            "/auth/device-authorization/approve",
            post(handlers::device_authorization_approve),
        )
        .route(
            "/auth/device-authorization/token",
            post(handlers::device_authorization_token),
        )
        .route("/auth/revoke", post(handlers::revoke))
        .route("/auth/sessions", get(handlers::list_sessions))
        .route("/auth/totp/status", post(handlers::totp_status))
        .route("/auth/totp/setup/start", post(handlers::totp_setup_start))
        .route("/auth/totp/setup/finish", post(handlers::totp_setup_finish))
        .route("/auth/totp/disable", post(handlers::totp_disable))
        .route(
            "/auth/account-recovery/codes/regenerate",
            post(handlers::account_recovery_codes_regenerate),
        )
        .route("/auth/passkey/add/start", post(handlers::passkey_add_start))
        .route(
            "/auth/passkey/add/finish",
            post(handlers::passkey_add_finish),
        )
        .route("/auth/passkeys", get(handlers::passkey_list))
        .route("/auth/passkey/update", post(handlers::passkey_update))
        .route("/auth/passkey/delete", post(handlers::passkey_delete))
        .route(
            "/auth/passkey/login/start",
            post(handlers::passkey_login_start),
        )
        .route(
            "/auth/passkey/login/finish",
            post(handlers::passkey_login_finish),
        )
}
