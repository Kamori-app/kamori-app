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

#[derive(serde::Serialize, serde::Deserialize)]
struct ReauthGrant {
    action: String,
    binding: crate::features::auth::repositories::AuthenticationBinding,
}

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
            Some(user.auth_generation),
        )
        .await
        .map_err(|_| bad_request("invalid OPAQUE reauthentication request"))?;
    state
        .state_store
        .put(
            &reauth_flow_key(opaque.flow_id),
            &rmp_serde::to_vec_named(&ReauthGrant {
                action: payload.action.as_str().to_owned(),
                binding: crate::features::auth::repositories::AuthenticationBinding {
                    user_id: user.id,
                    generation: user.auth_generation,
                    session_id: Some(principal.session_id),
                },
            })
            .map_err(internal_error)?,
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
    let proof = state
        .opaque
        .login_finish(
            &principal.username,
            payload.opaque_flow_id,
            &payload.opaque_finish_request,
        )
        .await
        .map_err(|_| unauthenticated("password reauthentication failed"))?;

    if proof.auth_generation != Some(user.auth_generation) {
        return Err(unauthenticated("authentication credentials have changed"));
    }
    let expected_action = state
        .state_store
        .take(&reauth_flow_key(payload.opaque_flow_id))
        .await
        .map_err(internal_error)?
        .ok_or_else(|| unauthenticated("reauthentication flow expired or was already used"))?;
    let grant: ReauthGrant = rmp_serde::from_slice(&expected_action).map_err(internal_error)?;
    if grant.action != payload.action.as_str()
        || grant.binding.user_id != user.id
        || grant.binding.generation != user.auth_generation
        || grant.binding.session_id != Some(principal.session_id)
    {
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

    let mut tx = state.pool.begin().await.map_err(internal_error)?;
    grant.binding.lock(&mut tx).await?;
    let reauth_token = state
        .issue_reauth_token(principal.user_id, &principal.username, principal.session_id)
        .map_err(internal_error)?;
    state
        .state_store
        .put(
            &reauth_token_key(&reauth_token),
            &rmp_serde::to_vec_named(&grant).map_err(internal_error)?,
            Duration::from_secs(5 * 60),
        )
        .await
        .map_err(internal_error)?;
    tx.commit().await.map_err(internal_error)?;
    Ok(ReauthFinishResponse { reauth_token })
}

pub(crate) async fn consume_reauth_token(
    state: &AppState,
    token: &str,
    user_id: Uuid,
    username: &str,
    expected_action: ReauthAction,
) -> Result<crate::features::auth::repositories::AuthenticationBinding, ApiError> {
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
    let action = state
        .state_store
        .take(&reauth_token_key(token))
        .await
        .map_err(internal_error)?
        .ok_or_else(|| unauthenticated("reauthentication proof expired or was already used"))?;
    let grant: ReauthGrant = rmp_serde::from_slice(&action).map_err(internal_error)?;
    if grant.action != expected_action.as_str()
        || grant.binding.user_id != user_id
        || grant.binding.session_id != Some(proof_session_id)
    {
        return Err(unauthenticated(
            "reauthentication proof has the wrong scope",
        ));
    }
    // Consumers retain this binding through their durable mutation transaction.
    let mut tx = state.pool.begin().await.map_err(internal_error)?;
    grant.binding.lock(&mut tx).await?;
    tx.commit().await.map_err(internal_error)?;
    Ok(grant.binding)
}

fn reauth_flow_key(flow_id: Uuid) -> String {
    format!("auth:reauth:flow:{flow_id}")
}

fn reauth_token_key(token: &str) -> String {
    let digest = Sha256::digest(token.as_bytes());
    format!("auth:reauth:token:{}", URL_SAFE_NO_PAD.encode(digest))
}
