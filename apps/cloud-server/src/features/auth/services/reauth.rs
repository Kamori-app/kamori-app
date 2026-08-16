//! Fresh OPAQUE and TOTP verification for destructive operations.

use axum::http::HeaderMap;
use time::OffsetDateTime;

use crate::{
    features::{
        auth::{
            dto::{
                ReauthFinishRequest, ReauthFinishResponse, ReauthStartRequest, ReauthStartResponse,
            },
            repositories::{consume_totp_backup_code, get_user_by_username},
        },
        common::{ApiError, authorize_principal, bad_request, internal_error, unauthenticated},
    },
    platform::{
        secret_box::decrypt_user_totp,
        security::auth::{TotpConfig, verify_totp},
        state::AppState,
    },
};

use super::support::{hash_account_recovery_code, normalize_recovery_code};

pub(crate) async fn start(
    state: &AppState,
    headers: &HeaderMap,
    payload: ReauthStartRequest,
) -> Result<ReauthStartResponse, ApiError> {
    if payload.opaque_start_request.is_empty() || payload.opaque_start_request.len() > 8 * 1024 {
        return Err(bad_request("opaque_start_request has invalid size"));
    }
    let principal = authorize_principal(state, headers).await?;
    let user = get_user_by_username(&state.pool, &principal.username).await?;
    if user.id != principal.user_id {
        return Err(unauthenticated("session identity mismatch"));
    }
    let password_file = user
        .opaque_record
        .ok_or_else(|| unauthenticated("password reauthentication unavailable"))?;
    let opaque = state
        .opaque
        .login_start(
            &principal.username,
            &payload.opaque_start_request,
            &password_file,
        )
        .await
        .map_err(internal_error)?;
    Ok(ReauthStartResponse {
        opaque_flow_id: opaque.flow_id,
        opaque_server_message: opaque.message,
        totp_required: state.config.enable_totp && user.totp_secret_ciphertext.is_some(),
    })
}

pub(crate) async fn finish(
    state: &AppState,
    headers: &HeaderMap,
    payload: ReauthFinishRequest,
) -> Result<ReauthFinishResponse, ApiError> {
    if payload.opaque_finish_request.is_empty() || payload.opaque_finish_request.len() > 8 * 1024 {
        return Err(bad_request("opaque_finish_request has invalid size"));
    }
    let principal = authorize_principal(state, headers).await?;
    let user = get_user_by_username(&state.pool, &principal.username).await?;
    if user.id != principal.user_id {
        return Err(unauthenticated("session identity mismatch"));
    }
    state
        .opaque
        .login_finish(
            &principal.username,
            payload.opaque_flow_id,
            &payload.opaque_finish_request,
        )
        .await
        .map_err(|_| unauthenticated("password reauthentication failed"))?;

    if state.config.enable_totp
        && let Some(ciphertext) = user.totp_secret_ciphertext.as_deref()
    {
        let secret = decrypt_user_totp(
            &state.config.auth_totp_kek,
            &user.id.to_string(),
            ciphertext,
        )
        .map_err(internal_error)?;
        let code = payload
            .totp_code
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| unauthenticated("two-factor code is required"))?;
        let totp_valid = verify_totp(
            &secret,
            code,
            OffsetDateTime::now_utc(),
            TotpConfig::default(),
        )
        .unwrap_or(false);
        let backup_valid = if totp_valid {
            false
        } else if let Ok(canonical) = normalize_recovery_code(code) {
            consume_totp_backup_code(
                &state.pool,
                user.id,
                &hash_account_recovery_code(&canonical),
            )
            .await?
        } else {
            false
        };
        if !totp_valid && !backup_valid {
            return Err(unauthenticated("invalid two-factor code"));
        }
    }

    let reauth_token = state
        .issue_reauth_token(principal.user_id, &principal.username)
        .map_err(internal_error)?;
    Ok(ReauthFinishResponse { reauth_token })
}
