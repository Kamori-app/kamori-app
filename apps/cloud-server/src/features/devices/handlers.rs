//! Device HTTP handlers.

use axum::{
    extract::{Path, State},
    http::HeaderMap,
};
use uuid::Uuid;

use crate::{
    features::common::{ApiError, MsgPack},
    platform::state::AppState,
};

use super::{
    dto::{
        ListDevicesResponse, RegisterDeviceRequest, RegisterDeviceResponse, RevokeDeviceResponse,
    },
    services,
};

pub async fn register(
    State(state): State<AppState>,
    headers: HeaderMap,
    MsgPack(request): MsgPack<RegisterDeviceRequest>,
) -> Result<MsgPack<RegisterDeviceResponse>, ApiError> {
    Ok(MsgPack(
        services::register(&state, &headers, request).await?,
    ))
}

pub async fn list(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<MsgPack<ListDevicesResponse>, ApiError> {
    Ok(MsgPack(services::list(&state, &headers).await?))
}

pub async fn revoke(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(device_id): Path<Uuid>,
) -> Result<MsgPack<RevokeDeviceResponse>, ApiError> {
    Ok(MsgPack(
        services::revoke(&state, &headers, device_id).await?,
    ))
}
