//! Service logic for TOTP enrollment/disable and recovery-code regeneration.

use axum::http::HeaderMap;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::{
    features::auth::dto::{
        AccountRecoveryCodesRegenerateResponse, TotpDisableRequest, TotpDisableResponse,
        TotpSetupFinishRequest, TotpSetupFinishResponse, TotpSetupStartResponse,
        TotpStatusResponse,
    },
    features::auth::repositories::{
        clear_totp_for_user, count_unused_recovery_codes, enable_totp_for_user_with_recovery_codes,
        get_user_totp_ciphertext_by_id, regenerate_recovery_codes_for_user,
    },
    features::common::{
        ApiError, authorize_principal, authorize_session, bad_request, unauthenticated,
    },
    platform::state::AppState,
    platform::{
        secret_box::{decrypt_user_totp, encrypt_user_totp},
        security::auth::{TotpConfig, verify_totp},
    },
};

use super::support::{
    build_totp_otpauth_uri, generate_account_recovery_code_batch, generate_totp_manual_entry_key,
    map_totp_disable_error, map_totp_setup_error, normalize_totp_manual_entry_key,
    totp_issuer_from_config,
};

pub(crate) async fn totp_status(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<TotpStatusResponse, ApiError> {
    let user_id = authorize_session(state, headers).await?;
    let enabled = get_user_totp_ciphertext_by_id(&state.pool, user_id)
        .await?
        .is_some();
    let recovery_codes_remaining = count_unused_recovery_codes(&state.pool, user_id).await?;

    Ok(TotpStatusResponse {
        available: state.config.enable_totp,
        enabled,
        recovery_codes_remaining,
    })
}

pub(crate) async fn totp_setup_start(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<TotpSetupStartResponse, ApiError> {
    if !state.config.enable_totp {
        return Err(bad_request("totp is disabled"));
    }

    let principal = authorize_principal(state, headers).await?;
    let user_id = principal.user_id;
    if get_user_totp_ciphertext_by_id(&state.pool, user_id)
        .await?
        .is_some()
    {
        return Err(bad_request("totp is already enabled"));
    }

    let username = principal.username;
    let manual_entry_key = generate_totp_manual_entry_key();
    let issuer = totp_issuer_from_config(&state.config);
    let otpauth_uri = build_totp_otpauth_uri(&issuer, &username, &manual_entry_key)?;

    Ok(TotpSetupStartResponse {
        manual_entry_key,
        otpauth_uri,
    })
}

pub(crate) async fn totp_setup_finish(
    state: &AppState,
    headers: &HeaderMap,
    payload: TotpSetupFinishRequest,
) -> Result<TotpSetupFinishResponse, ApiError> {
    if !state.config.enable_totp {
        return Err(bad_request("totp is disabled"));
    }

    let user_id = authorize_session(state, headers).await?;
    if get_user_totp_ciphertext_by_id(&state.pool, user_id)
        .await?
        .is_some()
    {
        return Err(bad_request("totp is already enabled"));
    }

    let manual_entry_key = normalize_totp_manual_entry_key(&payload.manual_entry_key)?;
    let code = payload.code.trim();
    if code.is_empty() {
        return Err(bad_request("code is required"));
    }

    let verified = verify_totp(
        &manual_entry_key,
        code,
        OffsetDateTime::now_utc(),
        TotpConfig::default(),
    )
    .map_err(map_totp_setup_error)?;
    if !verified {
        return Err(unauthenticated("invalid totp code"));
    }

    let generated_recovery_codes = generate_account_recovery_code_batch();
    let recovery_code_rows = generated_recovery_codes
        .iter()
        .map(|(_display, code_hash)| (Uuid::new_v4(), code_hash.clone()))
        .collect::<Vec<_>>();

    let totp_secret_ciphertext = encrypt_user_totp(
        &state.config.auth_totp_kek,
        &user_id.to_string(),
        &manual_entry_key,
    )
    .map_err(crate::features::common::internal_error)?;
    let enabled = enable_totp_for_user_with_recovery_codes(
        &state.pool,
        user_id,
        &totp_secret_ciphertext,
        &recovery_code_rows,
    )
    .await?;
    if !enabled {
        return Err(bad_request("totp is already enabled"));
    }

    let recovery_codes = generated_recovery_codes
        .into_iter()
        .map(|(display, _hash)| display)
        .collect();

    Ok(TotpSetupFinishResponse {
        enabled: true,
        recovery_codes,
    })
}

pub(crate) async fn totp_disable(
    state: &AppState,
    headers: &HeaderMap,
    payload: TotpDisableRequest,
) -> Result<TotpDisableResponse, ApiError> {
    let user_id = authorize_session(state, headers).await?;
    let existing_secret = get_user_totp_ciphertext_by_id(&state.pool, user_id).await?;

    let Some(ciphertext) = existing_secret else {
        return Ok(TotpDisableResponse { enabled: false });
    };
    let secret = decrypt_user_totp(
        &state.config.auth_totp_kek,
        &user_id.to_string(),
        &ciphertext,
    )
    .map_err(crate::features::common::internal_error)?;

    let code = payload
        .code
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| bad_request("code is required"))?;
    let verified = verify_totp(
        &secret,
        code,
        OffsetDateTime::now_utc(),
        TotpConfig::default(),
    )
    .map_err(map_totp_disable_error)?;
    if !verified {
        return Err(unauthenticated("invalid totp code"));
    }

    clear_totp_for_user(&state.pool, user_id).await?;
    Ok(TotpDisableResponse { enabled: false })
}

pub(crate) async fn account_recovery_codes_regenerate(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<AccountRecoveryCodesRegenerateResponse, ApiError> {
    let user_id = authorize_session(state, headers).await?;
    let generated_recovery_codes = generate_account_recovery_code_batch();
    let recovery_code_rows = generated_recovery_codes
        .iter()
        .map(|(_display, code_hash)| (Uuid::new_v4(), code_hash.clone()))
        .collect::<Vec<_>>();

    regenerate_recovery_codes_for_user(&state.pool, user_id, &recovery_code_rows).await?;
    let recovery_codes = generated_recovery_codes
        .into_iter()
        .map(|(display, _hash)| display)
        .collect();

    Ok(AccountRecoveryCodesRegenerateResponse { recovery_codes })
}
