//! Fresh OPAQUE and TOTP verification for destructive operations.

use axum::http::HeaderMap;
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use sha2::{Digest, Sha256};
use std::time::Duration;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::{
    features::{
        auth::{
            dto::{
                ReauthAction, ReauthFinishRequest, ReauthFinishResponse, ReauthStartRequest,
                ReauthStartResponse,
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
    crate::platform::rate_limit::enforce_credential_attempt(
        state,
        "opaque-reauth",
        principal.user_id.as_bytes(),
    )
    .await?;
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
            Some(&password_file),
        )
        .await
        .map_err(|_| bad_request("invalid OPAQUE reauthentication request"))?;
    state
        .state_store
        .put(
            &reauth_flow_key(opaque.flow_id),
            payload.action.as_str().as_bytes(),
            Duration::from_secs(5 * 60),
        )
        .await
        .map_err(internal_error)?;
    Ok(ReauthStartResponse {
        opaque_flow_id: opaque.flow_id,
        opaque_server_message: opaque.message,
        // Availability controls enrollment, never enforcement for an account
        // that already opted into a second factor.
        totp_required: user.totp_secret_ciphertext.is_some(),
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

    let expected_action = state
        .state_store
        .take(&reauth_flow_key(payload.opaque_flow_id))
        .await
        .map_err(internal_error)?
        .ok_or_else(|| unauthenticated("reauthentication flow expired or was already used"))?;
    if expected_action.as_slice() != payload.action.as_str().as_bytes() {
        return Err(unauthenticated("reauthentication action mismatch"));
    }

    if let Some(ciphertext) = user.totp_secret_ciphertext.as_deref() {
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
        .issue_reauth_token(principal.user_id, &principal.username, principal.session_id)
        .map_err(internal_error)?;
    state
        .state_store
        .put(
            &reauth_token_key(&reauth_token),
            payload.action.as_str().as_bytes(),
            Duration::from_secs(5 * 60),
        )
        .await
        .map_err(internal_error)?;
    Ok(ReauthFinishResponse { reauth_token })
}

pub(crate) async fn consume_reauth_token(
    state: &AppState,
    token: &str,
    user_id: Uuid,
    username: &str,
    expected_action: ReauthAction,
) -> Result<(), ApiError> {
    let proof = state
        .validate_token(token)
        .map_err(|_| unauthenticated("fresh reauthentication is required"))?;
    if proof.kind != crate::platform::jwt::TokenKind::Reauth
        || proof.user_id != user_id
        || proof.username.as_deref() != Some(username)
    {
        return Err(unauthenticated(
            "reauthentication proof does not match account",
        ));
    }
    let proof_session_id = proof
        .session_id
        .filter(|session_id| !session_id.is_nil())
        .ok_or_else(|| unauthenticated("reauthentication proof has no session binding"))?;
    if state.account_state_checks_enabled {
        let active: bool = sqlx::query_scalar(
            r#"
            SELECT EXISTS(
                SELECT 1 FROM refresh_tokens rt
                JOIN devices d
                  ON d.id = rt.device_id AND d.user_id = rt.user_id
                JOIN users u ON u.id = rt.user_id
                WHERE rt.id = $1 AND rt.user_id = $2
                  AND rt.revoked_at IS NULL AND rt.expires_at > now()
                  AND d.status = 'active'
                  AND u.deleted_at IS NULL AND u.suspended_at IS NULL
            )
            "#,
        )
        .bind(proof_session_id)
        .bind(user_id)
        .fetch_one(&state.pool)
        .await
        .map_err(internal_error)?;
        if !active {
            return Err(unauthenticated(
                "reauthentication session is no longer active",
            ));
        }
    }
    let action = state
        .state_store
        .take(&reauth_token_key(token))
        .await
        .map_err(internal_error)?
        .ok_or_else(|| unauthenticated("reauthentication proof expired or was already used"))?;
    if action.as_slice() != expected_action.as_str().as_bytes() {
        return Err(unauthenticated(
            "reauthentication proof has the wrong scope",
        ));
    }
    Ok(())
}

fn reauth_flow_key(flow_id: Uuid) -> String {
    format!("auth:reauth:flow:{flow_id}")
}

fn reauth_token_key(token: &str) -> String {
    let digest = Sha256::digest(token.as_bytes());
    format!("auth:reauth:token:{}", URL_SAFE_NO_PAD.encode(digest))
}
