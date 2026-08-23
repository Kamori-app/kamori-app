//! Security-space service rules.

use std::collections::HashSet;

use axum::http::HeaderMap;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::{
    features::{
        common::{
            ApiError, authorize_session, bad_request, conflict, internal_error, unauthorized,
        },
        workspaces::repositories::ensure_personal_workspace_for_user,
    },
    platform::state::AppState,
};

use super::{
    dto::{
        CreateSpaceRequest, CreateSpaceResponse, ListRecoveryKeyPackagesResponse,
        ListSpaceDevicesResponse, ListSpaceMembersResponse, ListSpacesResponse,
        PutDeviceKeyPackageRequest, PutDeviceKeyPackageResponse, PutRecoveryKeyPackageRequest,
        PutRecoveryKeyPackageResponse, RevokeSpaceMemberRequest, RevokeSpaceMemberResponse,
        RotateSpaceKeyRequest, RotateSpaceKeyResponse, SpaceLifecycleResponse,
    },
    repositories,
};

const ROTATION_SNAPSHOT_MAX_BYTES: usize = 25 * 1024 * 1024;
const ROTATION_SNAPSHOT_MAX_COUNT: usize = 10_000;
const MAX_DATABASE_KEY_EPOCH: u32 = i32::MAX as u32;

pub(crate) async fn create(
    state: &AppState,
    headers: &HeaderMap,
    request: CreateSpaceRequest,
) -> Result<CreateSpaceResponse, ApiError> {
    let user_id = authorize_session(state, headers).await?;
    if request.space_id.is_nil() || request.workspace_id.is_some_and(|id| id.is_nil()) {
        return Err(bad_request(
            "space_id and workspace_id must be non-nil UUIDs",
        ));
    }
    if request.encrypted_metadata.is_empty() || request.encrypted_metadata.len() > 64 * 1024 {
        return Err(bad_request("encrypted space metadata has invalid size"));
    }
    if request.device_key_packages.is_empty()
        || request.device_key_packages.iter().any(|package| {
            package.device_id.is_nil()
                || package.key_epoch != 1
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
    let result = repositories::create_space(&state.pool, user_id, workspace_id, &request)
        .await
        .map_err(internal_error)?;
    match result {
        repositories::CreateSpaceResult::Created(space) => Ok(CreateSpaceResponse { space }),
        repositories::CreateSpaceResult::WorkspaceAccessDenied => {
            Err(unauthorized("workspace access denied"))
        }
        repositories::CreateSpaceResult::InvalidDevicePackage => Err(bad_request(
            "device key package references an inactive or foreign device",
        )),
        repositories::CreateSpaceResult::IdConflict => Err(conflict("space_id is already in use")),
    }
}

pub(crate) async fn put_recovery_key_package(
    state: &AppState,
    headers: &HeaderMap,
    space_id: Uuid,
    request: PutRecoveryKeyPackageRequest,
) -> Result<PutRecoveryKeyPackageResponse, ApiError> {
    let user_id = authorize_session(state, headers).await?;
    if space_id.is_nil()
        || request.key_epoch == 0
        || request.key_epoch > MAX_DATABASE_KEY_EPOCH
        || !(49..=64 * 1024).contains(&request.encrypted_key_package.len())
    {
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

pub(crate) async fn list_recovery_key_packages(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<ListRecoveryKeyPackagesResponse, ApiError> {
    let user_id = authorize_session(state, headers).await?;
    let packages = repositories::list_recovery_key_packages(&state.pool, user_id)
        .await
        .map_err(internal_error)?;
    Ok(ListRecoveryKeyPackagesResponse { packages })
}

pub(crate) async fn put_device_key_package(
    state: &AppState,
    headers: &HeaderMap,
    space_id: Uuid,
    request: PutDeviceKeyPackageRequest,
) -> Result<PutDeviceKeyPackageResponse, ApiError> {
    let user_id = authorize_session(state, headers).await?;
    if space_id.is_nil()
        || request.package.device_id.is_nil()
        || request.package.key_epoch == 0
        || request.package.key_epoch > MAX_DATABASE_KEY_EPOCH
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
    let key_epoch = rotate_space_key(state, headers, space_id, Some(user_id), request).await?;
    Ok(RevokeSpaceMemberResponse {
        revoked: true,
        key_epoch,
    })
}

pub(crate) async fn rotate_key(
    state: &AppState,
    headers: &HeaderMap,
    space_id: Uuid,
    request: RotateSpaceKeyRequest,
) -> Result<RotateSpaceKeyResponse, ApiError> {
    let key_epoch = rotate_space_key(state, headers, space_id, None, request).await?;
    Ok(RotateSpaceKeyResponse {
        rotated: true,
        key_epoch,
    })
}

async fn rotate_space_key(
    state: &AppState,
    headers: &HeaderMap,
    space_id: Uuid,
    target_user_id: Option<Uuid>,
    request: RotateSpaceKeyRequest,
) -> Result<u32, ApiError> {
    let actor_id = authorize_session(state, headers).await?;
    if space_id.is_nil()
        || request.rotation_id.is_nil()
        || target_user_id.is_some_and(|user_id| user_id.is_nil())
    {
        return Err(bad_request("rotation ids must be non-nil UUIDs"));
    }
    if request.expected_key_epoch == 0
        || request.new_key_epoch == 0
        || request.expected_key_epoch > MAX_DATABASE_KEY_EPOCH
        || request.new_key_epoch > MAX_DATABASE_KEY_EPOCH
        || request.new_key_epoch != request.expected_key_epoch.saturating_add(1)
    {
        return Err(bad_request(
            "key epochs must be consecutive, positive, and fit PostgreSQL INTEGER",
        ));
    }
    if request.base_space_seq > i64::MAX as u64 {
        return Err(bad_request(
            "base_space_seq exceeds the supported cursor range",
        ));
    }
    if request.new_encrypted_metadata.is_empty() || request.new_encrypted_metadata.len() > 64 * 1024
    {
        return Err(bad_request("new encrypted space metadata has invalid size"));
    }
    if request.remaining_device_packages.iter().any(|package| {
        package.device_id.is_nil()
            || package.key_epoch != request.new_key_epoch
            || package.encrypted_key_package.is_empty()
            || package.encrypted_key_package.len() > 64 * 1024
    }) {
        return Err(bad_request("encrypted member key package has invalid size"));
    }
    if request.remaining_recovery_packages.iter().any(|package| {
        package.user_id.is_nil()
            || package.key_epoch != request.new_key_epoch
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

    validate_rotation_snapshots(space_id, request.new_key_epoch, &request.snapshots)?;
    let snapshot_streams = request
        .snapshots
        .iter()
        .map(|snapshot| snapshot.stream_id)
        .collect::<HashSet<_>>();
    let quarantined_streams = request
        .quarantined_streams
        .iter()
        .copied()
        .collect::<HashSet<_>>();
    if quarantined_streams.len() != request.quarantined_streams.len()
        || quarantined_streams.iter().any(Uuid::is_nil)
        || !snapshot_streams.is_disjoint(&quarantined_streams)
    {
        return Err(bad_request("quarantined snapshot streams are invalid"));
    }

    let request_hash = rotation_request_hash(space_id, target_user_id, &request);
    match repositories::check_rotation_retry(
        &state.pool,
        actor_id,
        space_id,
        target_user_id,
        request.new_key_epoch,
        request.rotation_id,
        &request_hash,
    )
    .await
    .map_err(internal_error)?
    {
        repositories::RotationRetryResult::Committed => {
            return Ok(request.new_key_epoch);
        }
        repositories::RotationRetryResult::Conflict => {
            return Err(conflict("rotation id is bound to another request"));
        }
        repositories::RotationRetryResult::AccessDenied => {
            return Err(unauthorized("space owner access required"));
        }
        repositories::RotationRetryResult::NotFound => {}
    }

    for snapshot in &request.snapshots {
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
            target_user_id,
            expected_key_epoch: request.expected_key_epoch,
            new_key_epoch: request.new_key_epoch,
            base_space_seq: request.base_space_seq,
            rotation_id: request.rotation_id,
            new_encrypted_metadata: &request.new_encrypted_metadata,
            packages: &request.remaining_device_packages,
            recovery_packages: &request.remaining_recovery_packages,
            snapshots: &request.snapshots,
            quarantined_streams: &request.quarantined_streams,
            request_hash: &request_hash,
        },
    )
    .await
    .map_err(internal_error)?
    {
        RevokeMemberResult::Revoked | RevokeMemberResult::AlreadyRevoked => {
            Ok(request.new_key_epoch)
        }
        RevokeMemberResult::AccessDenied => Err(unauthorized("space owner access required")),
        RevokeMemberResult::EpochConflict => Err(conflict("space key epoch changed")),
        RevokeMemberResult::RotationConflict => {
            Err(conflict("rotation id is bound to another request"))
        }
        RevokeMemberResult::TargetNotFound => Err(bad_request("active member not found")),
        RevokeMemberResult::CannotRevokeOwner => Err(bad_request("space owner cannot be revoked")),
        RevokeMemberResult::PackageCoverageMismatch => Err(bad_request(
            "new epoch must cover every remaining device, recovery identity, and active stream",
        )),
    }
}

fn validate_rotation_snapshots(
    space_id: Uuid,
    new_key_epoch: u32,
    snapshots: &[crypto_core_lib::operation_envelope::OperationEnvelopeV1],
) -> Result<(), ApiError> {
    if snapshots.len() > ROTATION_SNAPSHOT_MAX_COUNT {
        return Err(bad_request("rotation contains too many snapshots"));
    }

    let total_ciphertext_bytes = snapshots.iter().try_fold(0usize, |total, snapshot| {
        total.checked_add(snapshot.ciphertext.len())
    });
    if total_ciphertext_bytes.is_none_or(|total| total > ROTATION_SNAPSHOT_MAX_BYTES) {
        return Err(bad_request(
            "rotation snapshots exceed the total size limit",
        ));
    }

    let mut snapshot_streams = HashSet::new();
    let mut snapshot_ids = HashSet::new();
    for snapshot in snapshots {
        if snapshot.space_id != space_id
            || snapshot.stream_id.is_nil()
            || snapshot.client_op_id.is_nil()
            || snapshot.author_device_id.is_nil()
            || snapshot.key_epoch != new_key_epoch
            || snapshot.envelope_kind != crypto_core_lib::operation_envelope::EnvelopeKind::Snapshot
            || snapshot.nonce.len() != snapshot.cipher_suite.nonce_len()
            || snapshot.signature.len() != 64
            || snapshot.ciphertext.is_empty()
            || snapshot.ciphertext.len() > ROTATION_SNAPSHOT_MAX_BYTES
            || !snapshot_streams.insert(snapshot.stream_id)
            || !snapshot_ids.insert(snapshot.client_op_id)
        {
            return Err(bad_request("rotation snapshot envelope is invalid"));
        }
    }
    Ok(())
}

fn rotation_request_hash(
    space_id: Uuid,
    target_user_id: Option<Uuid>,
    request: &RotateSpaceKeyRequest,
) -> [u8; 32] {
    fn update_bytes(hasher: &mut Sha256, value: &[u8]) {
        hasher.update((value.len() as u64).to_be_bytes());
        hasher.update(value);
    }

    let mut hasher = Sha256::new();
    hasher.update(b"kamori.space-rotation-request.v2\0");
    hasher.update(space_id.as_bytes());
    match target_user_id {
        Some(target_user_id) => {
            hasher.update([1]);
            hasher.update(target_user_id.as_bytes());
        }
        None => hasher.update([0]),
    }
    hasher.update(request.rotation_id.as_bytes());
    hasher.update(request.expected_key_epoch.to_be_bytes());
    hasher.update(request.new_key_epoch.to_be_bytes());
    hasher.update(request.base_space_seq.to_be_bytes());
    update_bytes(&mut hasher, &request.new_encrypted_metadata);

    let mut device_packages = request.remaining_device_packages.iter().collect::<Vec<_>>();
    device_packages.sort_by_key(|package| package.device_id);
    for package in device_packages {
        hasher.update(package.device_id.as_bytes());
        hasher.update(package.key_epoch.to_be_bytes());
        update_bytes(&mut hasher, &package.encrypted_key_package);
    }

    let mut recovery_packages = request
        .remaining_recovery_packages
        .iter()
        .collect::<Vec<_>>();
    recovery_packages.sort_by_key(|package| package.user_id);
    for package in recovery_packages {
        hasher.update(package.user_id.as_bytes());
        hasher.update(package.key_epoch.to_be_bytes());
        update_bytes(&mut hasher, &package.encrypted_key_package);
    }

    let mut snapshots = request.snapshots.iter().collect::<Vec<_>>();
    snapshots.sort_by_key(|snapshot| (snapshot.stream_id, snapshot.client_op_id));
    for snapshot in snapshots {
        update_bytes(&mut hasher, &snapshot.canonical_signing_bytes());
        update_bytes(&mut hasher, &snapshot.signature);
    }
    let mut quarantined_streams = request.quarantined_streams.clone();
    quarantined_streams.sort_unstable();
    for stream_id in quarantined_streams {
        hasher.update(stream_id.as_bytes());
    }
    hasher.finalize().into()
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::spaces::dto::{DeviceKeyPackage, MemberRecoveryKeyPackage};
    use crypto_core_lib::operation_envelope::{
        EnvelopeCipherSuite, EnvelopeKind, OperationEnvelopeV1,
    };

    fn snapshot(space_id: Uuid, stream_id: Uuid, client_op_id: Uuid) -> OperationEnvelopeV1 {
        OperationEnvelopeV1 {
            space_id,
            stream_id,
            client_op_id,
            author_device_id: Uuid::from_u128(99),
            key_epoch: 2,
            envelope_kind: EnvelopeKind::Snapshot,
            cipher_suite: EnvelopeCipherSuite::Xchacha20Poly1305,
            nonce: vec![1; 24],
            ciphertext: vec![2; 32],
            signature: vec![3; 64],
        }
    }

    #[test]
    fn rotation_hash_is_order_independent_but_content_bound() {
        let space_id = Uuid::from_u128(1);
        let target_user_id = Uuid::from_u128(2);
        let rotation_id = Uuid::from_u128(3);
        let device_a = DeviceKeyPackage {
            device_id: Uuid::from_u128(4),
            key_epoch: 2,
            encrypted_key_package: vec![4; 49],
        };
        let device_b = DeviceKeyPackage {
            device_id: Uuid::from_u128(5),
            key_epoch: 2,
            encrypted_key_package: vec![5; 49],
        };
        let recovery_a = MemberRecoveryKeyPackage {
            user_id: Uuid::from_u128(6),
            key_epoch: 2,
            encrypted_key_package: vec![6; 49],
        };
        let recovery_b = MemberRecoveryKeyPackage {
            user_id: Uuid::from_u128(7),
            key_epoch: 2,
            encrypted_key_package: vec![7; 49],
        };
        let first_snapshot = snapshot(space_id, Uuid::from_u128(8), Uuid::from_u128(9));
        let second_snapshot = snapshot(space_id, Uuid::from_u128(10), Uuid::from_u128(11));
        let request = RevokeSpaceMemberRequest {
            rotation_id,
            expected_key_epoch: 1,
            new_key_epoch: 2,
            base_space_seq: 12,
            new_encrypted_metadata: vec![12],
            remaining_device_packages: vec![device_a.clone(), device_b.clone()],
            remaining_recovery_packages: vec![recovery_a.clone(), recovery_b.clone()],
            snapshots: vec![first_snapshot.clone(), second_snapshot.clone()],
            quarantined_streams: vec![],
        };
        let reordered = RevokeSpaceMemberRequest {
            remaining_device_packages: vec![device_b, device_a],
            remaining_recovery_packages: vec![recovery_b, recovery_a],
            snapshots: vec![second_snapshot, first_snapshot],
            ..request.clone()
        };
        assert_eq!(
            rotation_request_hash(space_id, Some(target_user_id), &request),
            rotation_request_hash(space_id, Some(target_user_id), &reordered)
        );

        let changed = RevokeSpaceMemberRequest {
            new_encrypted_metadata: vec![13],
            ..request.clone()
        };
        assert_ne!(
            rotation_request_hash(space_id, Some(target_user_id), &request),
            rotation_request_hash(space_id, Some(target_user_id), &changed)
        );
    }

    #[test]
    fn rotation_rejects_aggregate_snapshot_overflow() {
        let space_id = Uuid::from_u128(1);
        let mut first = snapshot(space_id, Uuid::from_u128(2), Uuid::from_u128(3));
        first.ciphertext = vec![1; ROTATION_SNAPSHOT_MAX_BYTES];
        let second = snapshot(space_id, Uuid::from_u128(4), Uuid::from_u128(5));

        assert!(validate_rotation_snapshots(space_id, 2, &[first]).is_ok());
        assert!(
            validate_rotation_snapshots(
                space_id,
                2,
                &[
                    snapshot(space_id, Uuid::from_u128(2), Uuid::from_u128(3)),
                    second,
                ]
            )
            .is_ok()
        );

        let mut over_limit = snapshot(space_id, Uuid::from_u128(6), Uuid::from_u128(7));
        over_limit.ciphertext = vec![1; ROTATION_SNAPSHOT_MAX_BYTES];
        assert!(
            validate_rotation_snapshots(
                space_id,
                2,
                &[
                    over_limit,
                    snapshot(space_id, Uuid::from_u128(8), Uuid::from_u128(9))
                ]
            )
            .is_err()
        );
    }

    #[test]
    fn rotation_rejects_nil_snapshot_ids() {
        let space_id = Uuid::from_u128(1);
        let mut invalid = snapshot(space_id, Uuid::from_u128(2), Uuid::from_u128(3));
        invalid.client_op_id = Uuid::nil();
        assert!(validate_rotation_snapshots(space_id, 2, &[invalid]).is_err());
    }
}
