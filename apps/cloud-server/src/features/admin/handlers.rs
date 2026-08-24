//! HTTP handlers for the isolated operator API.

use axum::{extract::State, http::HeaderMap};

use crate::{
    features::{
        admin::{
            dto::{
                AdminAuditResponse, AdminAuthFinishRequest, AdminAuthFinishResponse,
                AdminAuthStartRequest, AdminAuthStartResponse, AdminDashboardResponse,
                AdminMutationResponse, AdminPasskeyRenameRequest, AdminSecurityKeyAddFinishRequest,
                AdminSecurityKeyRegistrationFinishRequest, AdminSecurityKeyRegistrationResponse,
                AdminSecurityKeyRegistrationStartRequest, AdminSecurityKeyRemoveRequest,
                RuntimeSettingsResponse, SuspendAccountRequest, UpdateRuntimeSettingRequest,
            },
            services,
        },
        common::{ApiError, MsgPack},
    },
    platform::state::AppState,
};

pub async fn bootstrap_start(
    State(state): State<AppState>,
    MsgPack(payload): MsgPack<AdminSecurityKeyRegistrationStartRequest>,
) -> Result<MsgPack<AdminSecurityKeyRegistrationResponse>, ApiError> {
    Ok(MsgPack(services::bootstrap_start(&state, payload).await?))
}

pub async fn bootstrap_finish(
    State(state): State<AppState>,
    MsgPack(payload): MsgPack<AdminSecurityKeyRegistrationFinishRequest>,
) -> Result<MsgPack<AdminMutationResponse>, ApiError> {
    Ok(MsgPack(services::bootstrap_finish(&state, payload).await?))
}

pub async fn login_start(
    State(state): State<AppState>,
    MsgPack(payload): MsgPack<AdminAuthStartRequest>,
) -> Result<MsgPack<AdminAuthStartResponse>, ApiError> {
    Ok(MsgPack(services::login_start(&state, payload).await?))
}

pub async fn login_finish(
    State(state): State<AppState>,
    headers: HeaderMap,
    MsgPack(payload): MsgPack<AdminAuthFinishRequest>,
) -> Result<MsgPack<AdminAuthFinishResponse>, ApiError> {
    Ok(MsgPack(
        services::login_finish(&state, &headers, payload).await?,
    ))
}

pub async fn reauth_start(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<MsgPack<AdminAuthStartResponse>, ApiError> {
    Ok(MsgPack(services::reauth_start(&state, &headers).await?))
}

pub async fn reauth_finish(
    State(state): State<AppState>,
    headers: HeaderMap,
    MsgPack(payload): MsgPack<AdminAuthFinishRequest>,
) -> Result<MsgPack<AdminAuthFinishResponse>, ApiError> {
    Ok(MsgPack(
        services::reauth_finish(&state, &headers, payload).await?,
    ))
}

pub async fn logout(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<MsgPack<AdminMutationResponse>, ApiError> {
    Ok(MsgPack(services::logout(&state, &headers).await?))
}

pub async fn add_security_key_start(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<MsgPack<AdminSecurityKeyRegistrationResponse>, ApiError> {
    Ok(MsgPack(
        services::add_security_key_start(&state, &headers).await?,
    ))
}

pub async fn add_security_key_finish(
    State(state): State<AppState>,
    headers: HeaderMap,
    MsgPack(payload): MsgPack<AdminSecurityKeyAddFinishRequest>,
) -> Result<MsgPack<AdminMutationResponse>, ApiError> {
    Ok(MsgPack(
        services::add_security_key_finish(&state, &headers, payload).await?,
    ))
}

pub async fn remove_security_key(
    State(state): State<AppState>,
    headers: HeaderMap,
    MsgPack(payload): MsgPack<AdminSecurityKeyRemoveRequest>,
) -> Result<MsgPack<AdminMutationResponse>, ApiError> {
    Ok(MsgPack(
        services::remove_security_key(&state, &headers, payload).await?,
    ))
}

pub async fn rename_passkey(
    State(state): State<AppState>,
    headers: HeaderMap,
    MsgPack(payload): MsgPack<AdminPasskeyRenameRequest>,
) -> Result<MsgPack<AdminMutationResponse>, ApiError> {
    Ok(MsgPack(
        services::rename_passkey(&state, &headers, payload).await?,
    ))
}

pub async fn dashboard(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<MsgPack<AdminDashboardResponse>, ApiError> {
    Ok(MsgPack(services::dashboard(&state, &headers).await?))
}

pub async fn settings(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<MsgPack<RuntimeSettingsResponse>, ApiError> {
    Ok(MsgPack(services::settings(&state, &headers).await?))
}

pub async fn update_setting(
    State(state): State<AppState>,
    headers: HeaderMap,
    MsgPack(payload): MsgPack<UpdateRuntimeSettingRequest>,
) -> Result<MsgPack<AdminMutationResponse>, ApiError> {
    Ok(MsgPack(
        services::update_setting(&state, &headers, payload).await?,
    ))
}

pub async fn suspend(
    State(state): State<AppState>,
    headers: HeaderMap,
    MsgPack(payload): MsgPack<SuspendAccountRequest>,
) -> Result<MsgPack<AdminMutationResponse>, ApiError> {
    Ok(MsgPack(services::suspend(&state, &headers, payload).await?))
}

pub async fn audit(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<MsgPack<AdminAuditResponse>, ApiError> {
    Ok(MsgPack(services::audit(&state, &headers).await?))
}
