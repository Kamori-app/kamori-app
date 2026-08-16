//! HTTP handlers for password change and account recovery endpoints.

use axum::{extract::State, http::HeaderMap};

use crate::{
    features::auth::dto::{
        AccountRecoveryFinishRequest, AccountRecoveryFinishResponse, AccountRecoveryStartRequest,
        AccountRecoveryStartResponse, PasswordChangeFinishRequest, PasswordChangeFinishResponse,
        PasswordChangeStartRequest, PasswordChangeStartResponse,
    },
    features::auth::services as auth_services,
    features::common::{ApiError, MsgPack},
    platform::state::AppState,
};

pub async fn password_change_start(
    State(state): State<AppState>,
    headers: HeaderMap,
    MsgPack(payload): MsgPack<PasswordChangeStartRequest>,
) -> Result<MsgPack<PasswordChangeStartResponse>, ApiError> {
    Ok(MsgPack(
        auth_services::password_change_start(&state, &headers, payload).await?,
    ))
}

pub async fn password_change_finish(
    State(state): State<AppState>,
    headers: HeaderMap,
    MsgPack(payload): MsgPack<PasswordChangeFinishRequest>,
) -> Result<MsgPack<PasswordChangeFinishResponse>, ApiError> {
    Ok(MsgPack(
        auth_services::password_change_finish(&state, &headers, payload).await?,
    ))
}

pub async fn account_recovery_start(
    State(state): State<AppState>,
    MsgPack(payload): MsgPack<AccountRecoveryStartRequest>,
) -> Result<MsgPack<AccountRecoveryStartResponse>, ApiError> {
    Ok(MsgPack(
        auth_services::account_recovery_start(&state, payload).await?,
    ))
}

pub async fn account_recovery_finish(
    State(state): State<AppState>,
    MsgPack(payload): MsgPack<AccountRecoveryFinishRequest>,
) -> Result<MsgPack<AccountRecoveryFinishResponse>, ApiError> {
    Ok(MsgPack(
        auth_services::account_recovery_finish(&state, payload).await?,
    ))
}
