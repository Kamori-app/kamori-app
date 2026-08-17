//! Users HTTP handlers.

use axum::{extract::State, http::HeaderMap};

use crate::{
    features::{
        common::{ApiError, MsgPack, authorize_principal, authorize_session},
        users::{
            dto::{
                ConsentSettings, DeleteMeRequest, DeleteMeResponse, DeletionStatusResponse,
                UpdateConsentSettingsRequest,
            },
            services,
        },
    },
    platform::state::AppState,
};

pub async fn delete_me(
    State(state): State<AppState>,
    headers: HeaderMap,
    MsgPack(payload): MsgPack<DeleteMeRequest>,
) -> Result<MsgPack<DeleteMeResponse>, ApiError> {
    let principal = authorize_principal(&state, &headers).await?;
    Ok(MsgPack(
        services::delete_me(&state, principal.user_id, &principal.username, payload).await?,
    ))
}

pub async fn deletion_status(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<MsgPack<DeletionStatusResponse>, ApiError> {
    let user_id = authorize_session(&state, &headers).await?;
    Ok(MsgPack(
        services::account_deletion_status(&state, user_id).await?,
    ))
}

pub async fn get_consents(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<MsgPack<ConsentSettings>, ApiError> {
    let user_id = authorize_session(&state, &headers).await?;
    Ok(MsgPack(services::consent_settings(&state, user_id).await?))
}

pub async fn update_consents(
    State(state): State<AppState>,
    headers: HeaderMap,
    MsgPack(payload): MsgPack<UpdateConsentSettingsRequest>,
) -> Result<MsgPack<ConsentSettings>, ApiError> {
    let user_id = authorize_session(&state, &headers).await?;
    Ok(MsgPack(
        services::replace_consent_settings(&state, user_id, payload).await?,
    ))
}
