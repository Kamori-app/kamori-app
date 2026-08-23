//! HTTP handlers for TOTP and account-recovery-code endpoints.

use axum::{extract::State, http::HeaderMap};

use crate::{
    features::auth::dto::{
        AccountRecoveryCodesRegenerateRequest, AccountRecoveryCodesRegenerateResponse,
        TotpDisableRequest, TotpDisableResponse, TotpSetupFinishRequest, TotpSetupFinishResponse,
        TotpSetupStartRequest, TotpSetupStartResponse, TotpStatusRequest, TotpStatusResponse,
    },
    features::auth::services as auth_services,
    features::common::{ApiError, MsgPack},
    platform::state::AppState,
};

pub async fn totp_status(
    State(state): State<AppState>,
    headers: HeaderMap,
    MsgPack(_payload): MsgPack<TotpStatusRequest>,
) -> Result<MsgPack<TotpStatusResponse>, ApiError> {
    Ok(MsgPack(auth_services::totp_status(&state, &headers).await?))
}

pub async fn totp_setup_start(
    State(state): State<AppState>,
    headers: HeaderMap,
    MsgPack(payload): MsgPack<TotpSetupStartRequest>,
) -> Result<MsgPack<TotpSetupStartResponse>, ApiError> {
    Ok(MsgPack(
        auth_services::totp_setup_start(&state, &headers, payload).await?,
    ))
}

pub async fn totp_setup_finish(
    State(state): State<AppState>,
    headers: HeaderMap,
    MsgPack(payload): MsgPack<TotpSetupFinishRequest>,
) -> Result<MsgPack<TotpSetupFinishResponse>, ApiError> {
    Ok(MsgPack(
        auth_services::totp_setup_finish(&state, &headers, payload).await?,
    ))
}

pub async fn totp_disable(
    State(state): State<AppState>,
    headers: HeaderMap,
    MsgPack(payload): MsgPack<TotpDisableRequest>,
) -> Result<MsgPack<TotpDisableResponse>, ApiError> {
    Ok(MsgPack(
        auth_services::totp_disable(&state, &headers, payload).await?,
    ))
}

pub async fn account_recovery_codes_regenerate(
    State(state): State<AppState>,
    headers: HeaderMap,
    MsgPack(payload): MsgPack<AccountRecoveryCodesRegenerateRequest>,
) -> Result<MsgPack<AccountRecoveryCodesRegenerateResponse>, ApiError> {
    Ok(MsgPack(
        auth_services::account_recovery_codes_regenerate(&state, &headers, payload).await?,
    ))
}
