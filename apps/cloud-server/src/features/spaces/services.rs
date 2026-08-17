//! Security-space service rules.

use std::collections::HashSet;

use axum::http::HeaderMap;
use uuid::Uuid;

use crate::{
    features::{
        common::{
            ApiError, authorize_session, bad_request, conflict, internal_error, unauthorized,
        },
        workspaces::repositories::{
            ensure_personal_workspace_for_user, is_active_workspace_member,
        },
    },
    platform::state::AppState,
};

use super::{
    dto::{
        CreateSpaceRequest, CreateSpaceResponse, ListSpaceDevicesResponse,
        ListSpaceMembersResponse, ListSpacesResponse, PutDeviceKeyPackageRequest,
        PutDeviceKeyPackageResponse, PutRecoveryKeyPackageRequest, PutRecoveryKeyPackageResponse,
        RevokeSpaceMemberRequest, RevokeSpaceMemberResponse, SpaceLifecycleResponse,
    },
    repositories,
};

pub(crate) async fn create(
    state: &AppState,
    headers: &HeaderMap,
    request: CreateSpaceRequest,
) -> Result<CreateSpaceResponse, ApiError> {
    let user_id = authorize_session(state, headers).await?;
    if request.encrypted_metadata.is_empty() || request.encrypted_metadata.len() > 64 * 1024 {
        return Err(bad_request("encrypted space metadata has invalid size"));
    }
    if request.device_key_packages.is_empty()
        || request.device_key_packages.iter().any(|package| {
            package.key_epoch != 1
                || package.encrypted_key_package.is_empty()
                || package.encrypted_key_package.len() > 64 * 1024
        })
    {
        return Err(bad_request(
            "at least one valid epoch-1 device key package is required",
        ));
    }
    if !(49..=64 * 1024).contains(&request.encrypted_recovery_key_package.len()) {
        return Err(bad_request(
            "encrypted recovery key package has invalid size",
        ));
    }
    let unique_devices = request
        .device_key_packages
        .iter()
        .map(|package| package.device_id)
        .collect::<HashSet<_>>();
    if unique_devices.len() != request.device_key_packages.len() {
        return Err(bad_request("device key packages contain duplicates"));
    }

    let workspace_id = match request.workspace_id {
        Some(workspace_id) => workspace_id,
        None => ensure_personal_workspace_for_user(&state.pool, user_id).await?,
    };
    if !is_active_workspace_member(&state.pool, workspace_id, user_id).await? {
        return Err(unauthorized("workspace access denied"));
    }

    let space = repositories::create_space(&state.pool, user_id, workspace_id, &request)
        .await
        .map_err(internal_error)?;
    Ok(CreateSpaceResponse { space })
}

pub(crate) async fn put_recovery_key_package(
    state: &AppState,
    headers: &HeaderMap,
    space_id: Uuid,
    request: PutRecoveryKeyPackageRequest,
) -> Result<PutRecoveryKeyPackageResponse, ApiError> {
    let user_id = authorize_session(state, headers).await?;
    if request.key_epoch == 0 || !(49..=64 * 1024).contains(&request.encrypted_key_package.len()) {
        return Err(bad_request(
            "encrypted recovery key package has invalid size or epoch",
        ));
    }
    let stored = repositories::put_recovery_key_package(
        &state.pool,
        user_id,
        space_id,
        request.key_epoch,
        &request.encrypted_key_package,
    )
    .await
    .map_err(internal_error)?;
    if !stored {
        return Err(conflict("membership or key epoch is no longer active"));
    }
    Ok(PutRecoveryKeyPackageResponse { stored })
}

pub(crate) async fn put_device_key_package(
    state: &AppState,
    headers: &HeaderMap,
    space_id: Uuid,
    request: PutDeviceKeyPackageRequest,
) -> Result<PutDeviceKeyPackageResponse, ApiError> {
    let user_id = authorize_session(state, headers).await?;
    if request.package.key_epoch == 0
        || request.package.encrypted_key_package.is_empty()
        || request.package.encrypted_key_package.len() > 64 * 1024
    {
        return Err(bad_request("device key package has invalid size or epoch"));
    }
    let stored =
        repositories::put_device_key_package(&state.pool, user_id, space_id, &request.package)
            .await
            .map_err(internal_error)?;
    if !stored {
        return Err(conflict(
            "device, membership, or key epoch is no longer active",
        ));
    }
    Ok(PutDeviceKeyPackageResponse { stored })
}

pub(crate) async fn list_members(
    state: &AppState,
    headers: &HeaderMap,
    space_id: Uuid,
) -> Result<ListSpaceMembersResponse, ApiError> {
    let actor_id = authorize_session(state, headers).await?;
    let members = repositories::list_members(&state.pool, actor_id, space_id)
        .await
        .map_err(internal_error)?
        .ok_or_else(|| unauthorized("security-space read access denied"))?;
    Ok(ListSpaceMembersResponse { members })
}

pub(crate) async fn list_devices(
    state: &AppState,
    headers: &HeaderMap,
    space_id: Uuid,
) -> Result<ListSpaceDevicesResponse, ApiError> {
    let actor_id = authorize_session(state, headers).await?;
    let devices = repositories::list_space_devices(&state.pool, actor_id, space_id)
        .await
        .map_err(internal_error)?
        .ok_or_else(|| unauthorized("security-space read access denied"))?;
    Ok(ListSpaceDevicesResponse { devices })
}

pub(crate) async fn revoke_member(
    state: &AppState,
    headers: &HeaderMap,
    space_id: Uuid,
    user_id: Uuid,
    request: RevokeSpaceMemberRequest,
) -> Result<RevokeSpaceMemberResponse, ApiError> {
    let actor_id = authorize_session(state, headers).await?;
    if request.expected_key_epoch == 0 || request.new_key_epoch == 0 {
        return Err(bad_request("key epochs must be positive"));
    }
    if request.new_encrypted_metadata.is_empty() || request.new_encrypted_metadata.len() > 64 * 1024
    {
        return Err(bad_request("new encrypted space metadata has invalid size"));
    }
    if request.remaining_device_packages.iter().any(|package| {
        package.key_epoch != request.new_key_epoch
            || package.encrypted_key_package.is_empty()
            || package.encrypted_key_package.len() > 64 * 1024
    }) {
        return Err(bad_request("encrypted member key package has invalid size"));
    }
    if request.remaining_recovery_packages.iter().any(|package| {
        package.key_epoch != request.new_key_epoch
            || !(49..=64 * 1024).contains(&package.encrypted_key_package.len())
    }) {
        return Err(bad_request(
            "encrypted recovery key package has invalid size",
        ));
    }
    let unique_users = request
        .remaining_device_packages
        .iter()
        .map(|package| package.device_id)
        .collect::<HashSet<_>>();
    if unique_users.len() != request.remaining_device_packages.len() {
        return Err(bad_request(
            "remaining device key packages contain duplicates",
        ));
    }
    let unique_recovery_users = request
        .remaining_recovery_packages
        .iter()
        .map(|package| package.user_id)
        .collect::<HashSet<_>>();
    if unique_recovery_users.len() != request.remaining_recovery_packages.len() {
        return Err(bad_request(
            "remaining recovery packages contain duplicates",
        ));
    }

    let mut snapshot_streams = HashSet::new();
    let mut snapshot_ids = HashSet::new();
    for snapshot in &request.snapshots {
        if snapshot.space_id != space_id
            || snapshot.key_epoch != request.new_key_epoch
            || snapshot.envelope_kind != crypto_core_lib::operation_envelope::EnvelopeKind::Snapshot
            || snapshot.nonce.len() != snapshot.cipher_suite.nonce_len()
            || snapshot.signature.len() != 64
            || snapshot.ciphertext.is_empty()
            || snapshot.ciphertext.len() > 25 * 1024 * 1024
            || !snapshot_streams.insert(snapshot.stream_id)
            || !snapshot_ids.insert(snapshot.client_op_id)
        {
            return Err(bad_request("rotation snapshot envelope is invalid"));
        }
        let authorization = crate::features::operations::repositories::load_append_authorization(
            &state.pool,
            actor_id,
            snapshot,
        )
        .await
        .map_err(internal_error)?
        .ok_or_else(|| unauthorized("snapshot author device is not active"))?;
        if !authorization.can_write || authorization.current_key_epoch != request.expected_key_epoch
        {
            return Err(conflict("snapshot author authorization or epoch changed"));
        }
        snapshot
            .verify(&authorization.signing_public_key)
            .map_err(|_| bad_request("rotation snapshot signature is invalid"))?;
    }

    use repositories::RevokeMemberResult;
    match repositories::revoke_member_and_rotate(
        &state.pool,
        repositories::MemberRotation {
            actor_id,
            space_id,
            target_user_id: user_id,
            expected_key_epoch: request.expected_key_epoch,
            new_key_epoch: request.new_key_epoch,
            rotation_id: request.rotation_id,
            new_encrypted_metadata: &request.new_encrypted_metadata,
            packages: &request.remaining_device_packages,
            recovery_packages: &request.remaining_recovery_packages,
            snapshots: &request.snapshots,
        },
    )
    .await
    .map_err(internal_error)?
    {
        RevokeMemberResult::Revoked => Ok(RevokeSpaceMemberResponse {
            revoked: true,
            key_epoch: request.new_key_epoch,
        }),
        RevokeMemberResult::AccessDenied => Err(unauthorized("space owner access required")),
        RevokeMemberResult::EpochConflict => Err(conflict("space key epoch changed")),
        RevokeMemberResult::TargetNotFound => Err(bad_request("active member not found")),
        RevokeMemberResult::CannotRevokeOwner => Err(bad_request("space owner cannot be revoked")),
        RevokeMemberResult::PackageCoverageMismatch => Err(bad_request(
            "new epoch must cover every remaining device, recovery identity, and active stream",
        )),
    }
}

pub(crate) async fn list(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<ListSpacesResponse, ApiError> {
    let user_id = authorize_session(state, headers).await?;
    let spaces = repositories::list_spaces(&state.pool, user_id)
        .await
        .map_err(internal_error)?;
    Ok(ListSpacesResponse { spaces })
}

pub(crate) async fn list_trash(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<ListSpacesResponse, ApiError> {
    let user_id = authorize_session(state, headers).await?;
    let spaces = repositories::list_trashed_spaces(&state.pool, user_id)
        .await
        .map_err(internal_error)?;
    Ok(ListSpacesResponse { spaces })
}

pub(crate) async fn move_to_trash(
    state: &AppState,
    headers: &HeaderMap,
    space_id: Uuid,
) -> Result<SpaceLifecycleResponse, ApiError> {
    let user_id = authorize_session(state, headers).await?;
    let changed = repositories::move_to_trash(&state.pool, user_id, space_id)
        .await
        .map_err(internal_error)?;
    if !changed {
        return Err(unauthorized("space owner access required"));
    }
    Ok(SpaceLifecycleResponse { changed })
}

pub(crate) async fn restore_from_trash(
    state: &AppState,
    headers: &HeaderMap,
    space_id: Uuid,
) -> Result<SpaceLifecycleResponse, ApiError> {
    let user_id = authorize_session(state, headers).await?;
    let changed = repositories::restore_from_trash(&state.pool, user_id, space_id)
        .await
        .map_err(internal_error)?;
    if !changed {
        return Err(unauthorized("space owner access required"));
    }
    Ok(SpaceLifecycleResponse { changed })
}
