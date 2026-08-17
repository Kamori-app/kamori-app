//! Service logic for OPAQUE sign-up flow.

use uuid::Uuid;

use crate::{
    features::auth::dto::{
        SignupFinishRequest, SignupFinishResponse, SignupStartRequest, SignupStartResponse,
    },
    features::auth::repositories::{NewUser, UserAdmissionResult, insert_user_with_admission_cap},
    features::common::{ApiError, bad_request, conflict, internal_error, unauthorized},
    features::workspaces::repositories::ensure_personal_workspace_for_user,
    platform::state::AppState,
};

use super::support::{hash_data_recovery_verifier, normalize_username};

pub(crate) async fn signup_start(
    state: &AppState,
    payload: SignupStartRequest,
) -> Result<SignupStartResponse, ApiError> {
    if !crate::features::admin::services::effective_bool(
        state,
        "registration_enabled",
        state.config.registration_enabled,
    )
    .await?
    {
        return Err(unauthorized("registration is not open"));
    }
    let username = normalize_username(&payload.username)?;
    if payload.opaque_start_request.is_empty() || payload.opaque_start_request.len() > 8 * 1024 {
        return Err(bad_request("opaque_start_request has invalid size"));
    }
    let opaque_message = state
        .opaque
        .registration_start(&username, &payload.opaque_start_request)
        .await
        .map_err(internal_error)?;

    Ok(SignupStartResponse {
        opaque_server_message: opaque_message,
    })
}

pub(crate) async fn signup_finish(
    state: &AppState,
    payload: SignupFinishRequest,
) -> Result<SignupFinishResponse, ApiError> {
    if !crate::features::admin::services::effective_bool(
        state,
        "registration_enabled",
        state.config.registration_enabled,
    )
    .await?
    {
        return Err(unauthorized("registration is not open"));
    }
    let username = normalize_username(&payload.username)?;
    if payload.opaque_finish_request.is_empty()
        || payload.opaque_finish_request.len() > 8 * 1024
        || !(49..=64 * 1024).contains(&payload.encrypted_master_key.len())
        || payload.public_key_bundle.is_empty()
        || payload.public_key_bundle.len() > 64 * 1024
    {
        return Err(bad_request("signup key material has invalid size"));
    }
    let recovery_verifier_hash = hash_data_recovery_verifier(&payload.recovery_verifier)?;
    let password_file_bytes = state
        .opaque
        .registration_finish(&username, &payload.opaque_finish_request)
        .await
        .map_err(internal_error)?;

    let user_id = Uuid::new_v4();

    let admission = insert_user_with_admission_cap(
        &state.pool,
        NewUser {
            id: user_id,
            username: &username,
            opaque_record: &password_file_bytes,
            encrypted_master_key: &payload.encrypted_master_key,
            public_key_bundle: &payload.public_key_bundle,
            recovery_verifier_hash: &recovery_verifier_hash,
        },
        crate::features::admin::services::effective_u64(
            state,
            "beta_account_limit",
            state.config.beta_account_limit,
        )
        .await?,
    )
    .await
    .map_err(internal_error)?;
    if matches!(admission, UserAdmissionResult::CapacityReached) {
        return Err(conflict("public beta account capacity has been reached"));
    }

    ensure_personal_workspace_for_user(&state.pool, user_id).await?;

    Ok(SignupFinishResponse { user_id })
}
