//! Device service rules.

use axum::http::HeaderMap;
use uuid::Uuid;

use crate::{
    features::common::{ApiError, authorize_session, bad_request, internal_error},
    platform::state::AppState,
};

use super::{
    dto::{
        ListDevicesResponse, RegisterDeviceRequest, RegisterDeviceResponse, RevokeDeviceResponse,
    },
    repositories,
};

pub(crate) async fn register(
    state: &AppState,
    headers: &HeaderMap,
    request: RegisterDeviceRequest,
) -> Result<RegisterDeviceResponse, ApiError> {
    let user_id = authorize_session(state, headers).await?;
    if request.signing_public_key.len() != 32 || request.hpke_public_key.len() != 32 {
        return Err(bad_request("device public keys must be 32 bytes"));
    }
    if request.encrypted_name.is_empty() || request.encrypted_name.len() > 4096 {
        return Err(bad_request("encrypted device name has invalid size"));
    }
    let device = repositories::upsert_device(&state.pool, user_id, &request)
        .await
        .map_err(internal_error)?;
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
) -> Result<RevokeDeviceResponse, ApiError> {
    let user_id = authorize_session(state, headers).await?;
    let revoked = repositories::revoke_device(&state.pool, user_id, device_id)
        .await
        .map_err(internal_error)?;
    Ok(RevokeDeviceResponse { revoked })
}
