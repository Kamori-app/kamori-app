//! Service logic for invite-code creation and redemption.

use uuid::Uuid;

use crate::{
    features::{
        common::{ApiError, bad_request, internal_error, unauthorized},
        invites::{
            dto::{
                CreateInviteCodeRequest, CreateInviteCodeResponse, RedeemInviteCodeRequest,
                RedeemInviteCodeResponse,
            },
            repositories::{
                InviteCodeInsert, can_invite, insert_invite_code, redeem_invite_code_tx,
            },
        },
    },
    platform::state::AppState,
};

pub(crate) fn is_valid_invite_code_hash(invite_code_hash: &[u8]) -> bool {
    invite_code_hash.len() == 32
}

pub(crate) fn validate_ttl_minutes(ttl_minutes: u32) -> Option<i32> {
    if (15..=7 * 24 * 60).contains(&ttl_minutes) {
        i32::try_from(ttl_minutes).ok()
    } else {
        None
    }
}

pub(crate) async fn create_invite_code(
    state: &AppState,
    actor_id: Uuid,
    payload: CreateInviteCodeRequest,
) -> Result<CreateInviteCodeResponse, ApiError> {
    if !is_valid_invite_code_hash(&payload.invite_code_hash) {
        return Err(bad_request("invite_code_hash must be 32 bytes"));
    }

    let ttl_minutes = validate_ttl_minutes(payload.ttl_minutes)
        .ok_or_else(|| bad_request("ttl_minutes must be between 15 and 10080"))?;

    if payload.role == crate::features::spaces::dto::SpaceRole::Owner {
        return Err(bad_request("owner role cannot be granted by invite"));
    }
    if payload.encrypted_key_package.is_empty() || payload.encrypted_key_package.len() > 64 * 1024 {
        return Err(bad_request("encrypted_key_package has invalid size"));
    }
    if payload
        .encrypted_note
        .as_ref()
        .is_some_and(|note| note.len() > 64 * 1024)
    {
        return Err(bad_request("encrypted_note is too large"));
    }

    let can_manage = can_invite(&state.pool, payload.space_id, actor_id)
        .await
        .map_err(internal_error)?;
    if !can_manage {
        return Err(unauthorized("security-space invite access denied"));
    }

    let id = Uuid::new_v4();
    insert_invite_code(
        &state.pool,
        InviteCodeInsert {
            id,
            space_id: payload.space_id,
            created_by: actor_id,
            role: payload.role,
            code_hash: &payload.invite_code_hash,
            encrypted_key_package: &payload.encrypted_key_package,
            encrypted_note: payload.encrypted_note.as_ref().map(|bytes| bytes.as_ref()),
            ttl_minutes,
        },
    )
    .await
    .map_err(internal_error)?;

    Ok(CreateInviteCodeResponse { id })
}

pub(crate) async fn redeem_invite_code(
    state: &AppState,
    actor_id: Uuid,
    payload: RedeemInviteCodeRequest,
) -> Result<RedeemInviteCodeResponse, ApiError> {
    if !is_valid_invite_code_hash(&payload.invite_code_hash) {
        return Err(bad_request("invite_code_hash must be 32 bytes"));
    }

    let redeemed = redeem_invite_code_tx(&state.pool, &payload.invite_code_hash, actor_id)
        .await
        .map_err(|error| bad_request(&error.to_string()))?;

    Ok(RedeemInviteCodeResponse {
        space_id: redeemed.space_id,
        role: redeemed.role,
        key_epoch: redeemed.key_epoch,
        encrypted_key_package: redeemed.encrypted_key_package,
        encrypted_note: redeemed.encrypted_note,
    })
}

#[cfg(test)]
mod tests {
    use super::{is_valid_invite_code_hash, validate_ttl_minutes};

    #[test]
    fn invite_code_hash_validation_requires_sha256_size() {
        assert!(is_valid_invite_code_hash(&[0u8; 32]));
        assert!(!is_valid_invite_code_hash(&[0u8; 31]));
        assert!(!is_valid_invite_code_hash(&[0u8; 33]));
    }

    #[test]
    fn ttl_validation_bounds() {
        assert_eq!(validate_ttl_minutes(15), Some(15));
        assert_eq!(validate_ttl_minutes(10_080), Some(10_080));
        assert_eq!(validate_ttl_minutes(14), None);
        assert_eq!(validate_ttl_minutes(10_081), None);
    }
}
