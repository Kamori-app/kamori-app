//! Service logic for authenticated user account operations.

use uuid::Uuid;

use crate::features::auth::{dto::ReauthAction, services::consume_reauth_token};
use crate::{
    features::{
        common::{ApiError, bad_request, conflict, internal_error},
        users::{
            dto::{
                ConsentSettings, DeleteMeRequest, DeleteMeResponse, DeletionStatusResponse,
                UpdateConsentSettingsRequest,
            },
            repositories::{delete_user, deletion_status, get_consents, update_consents},
        },
    },
    platform::state::AppState,
};

pub(crate) async fn delete_me(
    state: &AppState,
    user_id: Uuid,
    username: &str,
    payload: DeleteMeRequest,
) -> Result<DeleteMeResponse, ApiError> {
    if payload.confirmation != format!("DELETE {username}") {
        return Err(bad_request("account deletion confirmation does not match"));
    }
    consume_reauth_token(
        state,
        &payload.reauth_token,
        user_id,
        username,
        ReauthAction::DeleteAccount,
    )
    .await?;
    let deleted = delete_user(&state.pool, user_id)
        .await
        .map_err(internal_error)?;
    if !deleted {
        return Err(conflict(
            "transfer or delete shared workspaces and security spaces before deleting the account",
        ));
    }
    Ok(DeleteMeResponse { deleted })
}

pub(crate) async fn account_deletion_status(
    state: &AppState,
    user_id: Uuid,
) -> Result<DeletionStatusResponse, ApiError> {
    deletion_status(&state.pool, user_id)
        .await
        .map_err(internal_error)
}

pub(crate) async fn consent_settings(
    state: &AppState,
    user_id: Uuid,
) -> Result<ConsentSettings, ApiError> {
    get_consents(&state.pool, user_id)
        .await
        .map_err(internal_error)
}

pub(crate) async fn replace_consent_settings(
    state: &AppState,
    user_id: Uuid,
    payload: UpdateConsentSettingsRequest,
) -> Result<ConsentSettings, ApiError> {
    update_consents(&state.pool, user_id, &payload)
        .await
        .map_err(internal_error)
}
