//! Device service rules.

use axum::http::HeaderMap;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::{
    features::common::{
        ApiError, authorize_enrollment_principal, authorize_session, bad_request, conflict,
        internal_error,
    },
    platform::state::AppState,
};

use super::{
    dto::{
        ListDevicesResponse, RegisterDeviceRequest, RegisterDeviceResponse, RevokeDeviceRequest,
        RevokeDeviceResponse,
    },
    repositories,
};

pub(crate) async fn register(
    state: &AppState,
    headers: &HeaderMap,
    request: RegisterDeviceRequest,
) -> Result<RegisterDeviceResponse, ApiError> {
    let principal = authorize_enrollment_principal(state, headers).await?;
    let user_id = principal.user_id;
    if request.device_id.is_nil() {
        return Err(bad_request("device_id must be a non-nil UUID"));
    }
    if request.signing_public_key.len() != 32 || request.hpke_public_key.len() != 32 {
        return Err(bad_request("device public keys must be 32 bytes"));
    }
    let signing_key: [u8; 32] = request
        .signing_public_key
        .as_slice()
        .try_into()
        .map_err(|_| bad_request("device signing public key is invalid"))?;
    let verifying_key = ed25519_dalek::VerifyingKey::from_bytes(&signing_key)
        .map_err(|_| bad_request("device signing public key is invalid"))?;
    if verifying_key.is_weak() {
        return Err(bad_request("device signing public key is weak"));
    }
    let hpke_key: [u8; 32] = request
        .hpke_public_key
        .as_slice()
        .try_into()
        .map_err(|_| bad_request("device HPKE public key is invalid"))?;
    crypto_core_lib::CryptoEngine::encrypt_group_key_for_peer(&[0; 32], &hpke_key)
        .map_err(|_| bad_request("device HPKE public key is invalid"))?;
    if request.encrypted_name.is_empty() || request.encrypted_name.len() > 4096 {
        return Err(bad_request("encrypted device name has invalid size"));
    }
    let mut enrollment_request = Vec::with_capacity(
        16 + request.signing_public_key.len()
            + request.hpke_public_key.len()
            + request.encrypted_name.len()
            + 32,
    );
    enrollment_request.extend_from_slice(b"kamori.device-enrollment.v1\0");
    enrollment_request.extend_from_slice(request.device_id.as_bytes());
    enrollment_request.extend_from_slice(&request.signing_public_key);
    enrollment_request.extend_from_slice(&request.hpke_public_key);
    enrollment_request.extend_from_slice(request.platform.as_db_value().as_bytes());
    enrollment_request.extend_from_slice(Sha256::digest(&request.encrypted_name).as_slice());
    crate::features::auth::services::device_enrollment::bind_request(
        state,
        &request.enrollment_token,
        user_id,
        &enrollment_request,
    )
    .await?;
    let device = repositories::upsert_device(&state.pool, user_id, principal.session_id, &request)
        .await
        .map_err(internal_error)?
        .ok_or_else(|| {
            conflict("device id is already bound to different keys, another account, or revoked")
        })?;
    Ok(RegisterDeviceResponse { device })
}

pub(crate) async fn list(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<ListDevicesResponse, ApiError> {
    let user_id = authorize_session(state, headers).await?;
    let devices = repositories::list_devices(&state.pool, user_id)
        .await
        .map_err(internal_error)?;
    Ok(ListDevicesResponse { devices })
}

pub(crate) async fn revoke(
    state: &AppState,
    headers: &HeaderMap,
    device_id: Uuid,
    request: RevokeDeviceRequest,
) -> Result<RevokeDeviceResponse, ApiError> {
    let principal = crate::features::common::authorize_principal(state, headers).await?;
    crate::features::auth::services::consume_reauth_token(
        state,
        &request.reauth_token,
        principal.user_id,
        &principal.username,
        crate::features::auth::dto::ReauthAction::SecuritySettings,
    )
    .await?;
    let user_id = principal.user_id;
    let revoked = repositories::revoke_device(&state.pool, user_id, device_id)
        .await
        .map_err(internal_error)?;
    Ok(RevokeDeviceResponse { revoked })
}
