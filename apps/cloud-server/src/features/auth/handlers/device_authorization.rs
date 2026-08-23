use axum::{extract::State, http::HeaderMap};

use crate::{
    features::auth::{dto::*, services},
    features::common::{ApiError, MsgPack},
    platform::state::AppState,
};

pub async fn start(
    State(state): State<AppState>,
    MsgPack(request): MsgPack<DeviceAuthorizationStartRequest>,
) -> Result<MsgPack<DeviceAuthorizationStartResponse>, ApiError> {
    Ok(MsgPack(
        services::device_authorization_start(&state, request).await?,
    ))
}

pub async fn inspect(
    State(state): State<AppState>,
    headers: HeaderMap,
    MsgPack(request): MsgPack<DeviceAuthorizationInspectRequest>,
) -> Result<MsgPack<DeviceAuthorizationInspectResponse>, ApiError> {
    Ok(MsgPack(
        services::device_authorization_inspect(&state, &headers, request).await?,
    ))
}

pub async fn approve(
    State(state): State<AppState>,
    headers: HeaderMap,
    MsgPack(request): MsgPack<DeviceAuthorizationApproveRequest>,
) -> Result<MsgPack<DeviceAuthorizationApproveResponse>, ApiError> {
    Ok(MsgPack(
        services::device_authorization_approve(&state, &headers, request).await?,
    ))
}

pub async fn token(
    State(state): State<AppState>,
    headers: HeaderMap,
    MsgPack(request): MsgPack<DeviceAuthorizationTokenRequest>,
) -> Result<MsgPack<DeviceAuthorizationTokenResponse>, ApiError> {
    Ok(MsgPack(
        services::device_authorization_token(&state, &headers, request).await?,
    ))
}
