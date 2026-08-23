//! Service logic for OPAQUE sign-up flow.

use serde::Deserialize;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::{
    features::auth::dto::{
        SignupFinishRequest, SignupFinishResponse, SignupStartRequest, SignupStartResponse,
    },
    features::auth::repositories::{
        NewUser, UserAdmissionResult, find_signup_completion,
        insert_user_with_personal_workspace_and_admission_cap,
    },
    features::common::{ApiError, bad_request, conflict, internal_error, unauthorized},
    platform::state::AppState,
};

use super::support::{hash_data_recovery_verifier, normalize_username};

#[derive(Deserialize)]
struct AccountPublicKeyBundleV2 {
    version: u8,
    #[serde(with = "serde_bytes")]
    account_recovery_public_key: Vec<u8>,
}

fn validate_public_key_bundle(encoded: &[u8]) -> Result<(), ApiError> {
    let bundle: AccountPublicKeyBundleV2 =
        rmp_serde::from_slice(encoded).map_err(|_| bad_request("public_key_bundle is invalid"))?;
    if bundle.version != 2 || bundle.account_recovery_public_key.len() != 32 {
        return Err(bad_request("public_key_bundle is invalid"));
    }
    let public_key: [u8; 32] = bundle
        .account_recovery_public_key
        .try_into()
        .map_err(|_| bad_request("public_key_bundle is invalid"))?;
    crypto_core_lib::CryptoEngine::encrypt_group_key_for_peer(&[0_u8; 32], &public_key)
        .map_err(|_| bad_request("public_key_bundle is invalid"))?;
    Ok(())
}

fn signup_request_hash(username: &str, payload: &SignupFinishRequest) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(b"kamori.signup-finish.v1\0");
    for value in [
        username.as_bytes(),
        payload.opaque_finish_request.as_slice(),
        payload.encrypted_master_key.as_slice(),
        payload.public_key_bundle.as_slice(),
        payload.recovery_verifier.as_slice(),
    ] {
        hasher.update((value.len() as u64).to_be_bytes());
        hasher.update(value);
    }
    hasher.finalize().into()
}

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
    crate::platform::rate_limit::enforce_credential_attempt(
        state,
        "opaque-signup",
        username.as_bytes(),
    )
    .await?;
    if payload.opaque_start_request.is_empty() || payload.opaque_start_request.len() > 8 * 1024 {
        return Err(bad_request("opaque_start_request has invalid size"));
    }
    let opaque_message = state
        .opaque
        .registration_start(&username, &payload.opaque_start_request)
        .await
        .map_err(|_| bad_request("invalid OPAQUE registration request"))?;

    Ok(SignupStartResponse {
        opaque_server_message: opaque_message,
    })
}

pub(crate) async fn signup_finish(
    state: &AppState,
    payload: SignupFinishRequest,
) -> Result<SignupFinishResponse, ApiError> {
    let username = normalize_username(&payload.username)?;
    if payload.signup_request_id.is_nil() {
        return Err(bad_request("signup_request_id must be a non-nil UUID"));
    }
    if payload.opaque_finish_request.is_empty()
        || payload.opaque_finish_request.len() > 8 * 1024
        || !(49..=64 * 1024).contains(&payload.encrypted_master_key.len())
        || payload.public_key_bundle.is_empty()
        || payload.public_key_bundle.len() > 64 * 1024
    {
        return Err(bad_request("signup key material has invalid size"));
    }
    validate_public_key_bundle(&payload.public_key_bundle)?;
    let recovery_verifier_hash = hash_data_recovery_verifier(&payload.recovery_verifier)?;
    let request_hash = signup_request_hash(&username, &payload);
    if let Some(completion) = find_signup_completion(
        &state.pool,
        payload.signup_request_id,
        &username,
        &request_hash,
    )
    .await
    .map_err(internal_error)?
    {
        return match completion {
            UserAdmissionResult::Duplicate(user_id) => Ok(SignupFinishResponse { user_id }),
            UserAdmissionResult::IdempotencyConflict => Err(conflict(
                "signup_request_id was already used for different data",
            )),
            UserAdmissionResult::Inserted
            | UserAdmissionResult::CapacityReached
            | UserAdmissionResult::UsernameExists => {
                unreachable!("completion lookup only returns terminal idempotency states")
            }
        };
    }
    if !crate::features::admin::services::effective_bool(
        state,
        "registration_enabled",
        state.config.registration_enabled,
    )
    .await?
    {
        return Err(unauthorized("registration is not open"));
    }
    let password_file_bytes = state
        .opaque
        .registration_finish(&username, &payload.opaque_finish_request)
        .await
        .map_err(|_| bad_request("invalid OPAQUE registration finish"))?;

    let user_id = Uuid::new_v4();

    let admission = insert_user_with_personal_workspace_and_admission_cap(
        &state.pool,
        NewUser {
            id: user_id,
            username: &username,
            opaque_record: &password_file_bytes,
            encrypted_master_key: &payload.encrypted_master_key,
            public_key_bundle: &payload.public_key_bundle,
            recovery_verifier_hash: &recovery_verifier_hash,
            signup_request_id: payload.signup_request_id,
            signup_request_hash: &request_hash,
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
    if matches!(&admission, UserAdmissionResult::CapacityReached) {
        return Err(conflict("public beta account capacity has been reached"));
    }
    if matches!(&admission, UserAdmissionResult::UsernameExists) {
        return Err(conflict("username is already registered"));
    }
    if matches!(&admission, UserAdmissionResult::IdempotencyConflict) {
        return Err(conflict(
            "signup_request_id was already used for different data",
        ));
    }
    let user_id = match admission {
        UserAdmissionResult::Inserted => user_id,
        UserAdmissionResult::Duplicate(existing_user_id) => existing_user_id,
        UserAdmissionResult::CapacityReached
        | UserAdmissionResult::UsernameExists
        | UserAdmissionResult::IdempotencyConflict => unreachable!("handled above"),
    };
    Ok(SignupFinishResponse { user_id })
}
