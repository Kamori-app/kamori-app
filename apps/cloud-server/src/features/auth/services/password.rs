//! Service logic for password change and account recovery reset.

use sha2::{Digest, Sha256};
use time::{Duration, OffsetDateTime};

use crate::{
    features::auth::dto::{
        AccountRecoveryFinishRequest, AccountRecoveryFinishResponse, AccountRecoveryStartRequest,
        AccountRecoveryStartResponse, PasswordChangeFinishRequest, PasswordChangeFinishResponse,
        PasswordChangeStartRequest, PasswordChangeStartResponse,
    },
    features::auth::repositories::{
        AccountRecoveryReset, apply_account_recovery_reset, create_account_recovery_attempt,
        find_account_recovery_attempt_user, find_user_for_data_recovery,
        update_user_password_file_and_revoke_refresh_sessions,
    },
    features::common::{
        ApiError, authorize_principal, bad_request, internal_error, unauthenticated,
    },
    platform::state::AppState,
};

use super::consume_reauth_token;
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
        .map_err(|_| bad_request("invalid OPAQUE password-change request"))?;

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
        .map_err(|_| bad_request("invalid OPAQUE password-change finish"))?;

    consume_reauth_token(
        state,
        &payload.reauth_token,
        user_id,
        &username,
        crate::features::auth::dto::ReauthAction::ChangePassword,
    )
    .await?;

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
    crate::platform::rate_limit::enforce_credential_attempt(
        state,
        "account-recovery",
        username.as_bytes(),
    )
    .await?;
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
        .map_err(|_| unauthenticated("invalid recovery credentials"))?;
    let recovery_token = state
        .issue_account_recovery_token(user_id, &username)
        .map_err(internal_error)?;
    let token_hash: [u8; 32] = Sha256::digest(recovery_token.as_bytes()).into();
    create_account_recovery_attempt(
        &state.pool,
        &token_hash,
        user_id,
        OffsetDateTime::now_utc()
            + Duration::seconds(state.config.jwt_account_recovery_ttl_seconds.max(1)),
    )
    .await?;

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
    if !(32..=4096).contains(&payload.recovery_token.len()) {
        return Err(unauthenticated("invalid or expired account recovery token"));
    }
    let token_hash: [u8; 32] = Sha256::digest(payload.recovery_token.as_bytes()).into();
    let user_id = find_account_recovery_attempt_user(&state.pool, &token_hash)
        .await?
        .ok_or_else(|| unauthenticated("invalid or expired account recovery token"))?;
    let password_file_bytes = state
        .opaque
        .registration_finish("", &payload.opaque_finish_request)
        .await
        .map_err(|_| bad_request("invalid OPAQUE account-recovery finish"))?;
    let mut request_hasher = Sha256::new();
    request_hasher.update(b"kamori.account-recovery-finish.v1\0");
    request_hasher.update((payload.opaque_finish_request.len() as u64).to_be_bytes());
    request_hasher.update(&payload.opaque_finish_request);
    request_hasher.update((payload.encrypted_master_key.len() as u64).to_be_bytes());
    request_hasher.update(&payload.encrypted_master_key);
    let request_hash: [u8; 32] = request_hasher.finalize().into();
    apply_account_recovery_reset(
        &state.pool,
        AccountRecoveryReset {
            user_id,
            token_hash: &token_hash,
            request_hash: &request_hash,
            opaque_record: &password_file_bytes,
            encrypted_master_key: &payload.encrypted_master_key,
        },
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
