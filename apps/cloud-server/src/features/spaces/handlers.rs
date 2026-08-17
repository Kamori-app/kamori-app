//! Security-space HTTP handlers.

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
        CreateSpaceRequest, CreateSpaceResponse, ListSpaceDevicesResponse,
        ListSpaceMembersResponse, ListSpacesResponse, PutDeviceKeyPackageRequest,
        PutDeviceKeyPackageResponse, PutRecoveryKeyPackageRequest, PutRecoveryKeyPackageResponse,
        RevokeSpaceMemberRequest, RevokeSpaceMemberResponse, SpaceLifecycleResponse,
    },
    services,
};

pub async fn create(
    State(state): State<AppState>,
    headers: HeaderMap,
    MsgPack(request): MsgPack<CreateSpaceRequest>,
) -> Result<MsgPack<CreateSpaceResponse>, ApiError> {
    Ok(MsgPack(services::create(&state, &headers, request).await?))
}

pub async fn put_recovery_key_package(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(space_id): Path<Uuid>,
    MsgPack(request): MsgPack<PutRecoveryKeyPackageRequest>,
) -> Result<MsgPack<PutRecoveryKeyPackageResponse>, ApiError> {
    Ok(MsgPack(
        services::put_recovery_key_package(&state, &headers, space_id, request).await?,
    ))
}

pub async fn put_device_key_package(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(space_id): Path<Uuid>,
    MsgPack(request): MsgPack<PutDeviceKeyPackageRequest>,
) -> Result<MsgPack<PutDeviceKeyPackageResponse>, ApiError> {
    Ok(MsgPack(
        services::put_device_key_package(&state, &headers, space_id, request).await?,
    ))
}

pub async fn list_members(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(space_id): Path<Uuid>,
) -> Result<MsgPack<ListSpaceMembersResponse>, ApiError> {
    Ok(MsgPack(
        services::list_members(&state, &headers, space_id).await?,
    ))
}

pub async fn list_devices(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(space_id): Path<Uuid>,
) -> Result<MsgPack<ListSpaceDevicesResponse>, ApiError> {
    Ok(MsgPack(
        services::list_devices(&state, &headers, space_id).await?,
    ))
}

pub async fn revoke_member(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((space_id, user_id)): Path<(Uuid, Uuid)>,
    MsgPack(request): MsgPack<RevokeSpaceMemberRequest>,
) -> Result<MsgPack<RevokeSpaceMemberResponse>, ApiError> {
    Ok(MsgPack(
        services::revoke_member(&state, &headers, space_id, user_id, request).await?,
    ))
}

pub async fn list(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<MsgPack<ListSpacesResponse>, ApiError> {
    Ok(MsgPack(services::list(&state, &headers).await?))
}

pub async fn list_trash(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<MsgPack<ListSpacesResponse>, ApiError> {
    Ok(MsgPack(services::list_trash(&state, &headers).await?))
}

pub async fn move_to_trash(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(space_id): Path<Uuid>,
) -> Result<MsgPack<SpaceLifecycleResponse>, ApiError> {
    Ok(MsgPack(
        services::move_to_trash(&state, &headers, space_id).await?,
    ))
}

pub async fn restore_from_trash(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(space_id): Path<Uuid>,
) -> Result<MsgPack<SpaceLifecycleResponse>, ApiError> {
    Ok(MsgPack(
        services::restore_from_trash(&state, &headers, space_id).await?,
    ))
}
