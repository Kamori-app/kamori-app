//! Service logic for password change and account recovery reset.

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use sha2::{Digest, Sha256};
use std::time::Duration;
use uuid::Uuid;

use crate::{
    features::auth::dto::{
        AccountRecoveryFinishRequest, AccountRecoveryFinishResponse, AccountRecoveryStartRequest,
        AccountRecoveryStartResponse, PasswordChangeFinishRequest, PasswordChangeFinishResponse,
        PasswordChangeStartRequest, PasswordChangeStartResponse,
    },
    features::auth::repositories::{
        apply_account_recovery_reset, find_user_for_data_recovery,
        update_user_password_file_and_revoke_refresh_sessions,
    },
    features::common::{
        ApiError, authorize_principal, bad_request, internal_error, unauthenticated,
    },
    platform::jwt::TokenKind,
    platform::state::AppState,
};

use super::support::{hash_data_recovery_verifier, normalize_username};

pub(crate) async fn password_change_start(
    state: &AppState,
    headers: &axum::http::HeaderMap,
    payload: PasswordChangeStartRequest,
) -> Result<PasswordChangeStartResponse, ApiError> {
    if payload.opaque_start_request.is_empty() || payload.opaque_start_request.len() > 8 * 1024 {
        return Err(bad_request("opaque_start_request has invalid size"));
    }
    let principal = authorize_principal(state, headers).await?;
    let username = principal.username;

    let opaque_message = state
        .opaque
        .registration_start(&username, &payload.opaque_start_request)
        .await
        .map_err(internal_error)?;

    Ok(PasswordChangeStartResponse {
        opaque_server_message: opaque_message,
    })
}

pub(crate) async fn password_change_finish(
    state: &AppState,
    headers: &axum::http::HeaderMap,
    payload: PasswordChangeFinishRequest,
) -> Result<PasswordChangeFinishResponse, ApiError> {
    if payload.opaque_finish_request.is_empty()
        || payload.opaque_finish_request.len() > 8 * 1024
        || !(49..=64 * 1024).contains(&payload.encrypted_master_key.len())
    {
        return Err(bad_request("password change key material has invalid size"));
    }
    let principal = authorize_principal(state, headers).await?;
    let user_id = principal.user_id;
    let username = principal.username;

    let password_file_bytes = state
        .opaque
        .registration_finish(&username, &payload.opaque_finish_request)
        .await
        .map_err(internal_error)?;

    update_user_password_file_and_revoke_refresh_sessions(
        &state.pool,
        user_id,
        &password_file_bytes,
        &payload.encrypted_master_key,
    )
    .await?;

    Ok(PasswordChangeFinishResponse { changed: true })
}

pub(crate) async fn account_recovery_start(
    state: &AppState,
    payload: AccountRecoveryStartRequest,
) -> Result<AccountRecoveryStartResponse, ApiError> {
    if payload.opaque_start_request.is_empty() || payload.opaque_start_request.len() > 8 * 1024 {
        return Err(unauthenticated("invalid recovery credentials"));
    }
    let username = normalize_username(&payload.username)
        .map_err(|_| unauthenticated("invalid recovery credentials"))?;
    let recovery_verifier_hash = hash_data_recovery_verifier(&payload.recovery_verifier)
        .map_err(|_| unauthenticated("invalid recovery credentials"))?;
    let Some(user_id) =
        find_user_for_data_recovery(&state.pool, &username, &recovery_verifier_hash).await?
    else {
        return Err(unauthenticated("invalid recovery credentials"));
    };

    let opaque_message = state
        .opaque
        .registration_start(&username, &payload.opaque_start_request)
        .await
        .map_err(internal_error)?;
    let recovery_token = state
        .issue_account_recovery_token(user_id, &username)
        .map_err(internal_error)?;
    state
        .state_store
        .put(
            &recovery_token_state_key(&recovery_token),
            user_id.as_bytes(),
            Duration::from_secs(state.config.jwt_account_recovery_ttl_seconds.max(1) as u64),
        )
        .await
        .map_err(internal_error)?;

    Ok(AccountRecoveryStartResponse {
        opaque_server_message: opaque_message,
        recovery_token,
    })
}

pub(crate) async fn account_recovery_finish(
    state: &AppState,
    payload: AccountRecoveryFinishRequest,
) -> Result<AccountRecoveryFinishResponse, ApiError> {
    if payload.opaque_finish_request.is_empty()
        || payload.opaque_finish_request.len() > 8 * 1024
        || !(49..=64 * 1024).contains(&payload.encrypted_master_key.len())
    {
        return Err(bad_request(
            "account recovery key material has invalid size",
        ));
    }
    let (user_id, username) =
        validate_account_recovery_token_for_user(state, &payload.recovery_token)?;
    let consumed_user_id = state
        .state_store
        .take(&recovery_token_state_key(&payload.recovery_token))
        .await
        .map_err(internal_error)?
        .ok_or_else(|| unauthenticated("account recovery token was already used or expired"))?;
    if consumed_user_id.as_slice() != user_id.as_bytes() {
        return Err(unauthenticated("invalid account recovery token"));
    }
    let password_file_bytes = state
        .opaque
        .registration_finish(&username, &payload.opaque_finish_request)
        .await
        .map_err(internal_error)?;

    apply_account_recovery_reset(
        &state.pool,
        user_id,
        &password_file_bytes,
        &payload.encrypted_master_key,
    )
    .await?;

    let space_key_packages =
        crate::features::spaces::repositories::list_recovery_key_packages(&state.pool, user_id)
            .await
            .map_err(internal_error)?;

    Ok(AccountRecoveryFinishResponse {
        changed: true,
        totp_disabled: true,
        space_key_packages,
    })
}

fn recovery_token_state_key(token: &str) -> String {
    let digest = Sha256::digest(token.as_bytes());
    format!("auth:account-recovery:{}", URL_SAFE_NO_PAD.encode(digest))
}

fn validate_account_recovery_token_for_user(
    state: &AppState,
    token: &str,
) -> Result<(Uuid, String), ApiError> {
    let claims = state.validate_token(token).map_err(internal_error)?;
    if claims.kind != TokenKind::AccountRecovery {
        return Err(unauthenticated("invalid account recovery token"));
    }
    let username = claims
        .username
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| unauthenticated("invalid account recovery token"))?;
    Ok((claims.user_id, username))
}
