//! Short-lived, request-bound authorization for durable device enrollment.

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use rand::RngExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::time::Duration;
use uuid::Uuid;

use crate::{
    features::common::{ApiError, internal_error, unauthenticated},
    platform::state::AppState,
};

const ENROLLMENT_TTL: Duration = Duration::from_secs(5 * 60);

#[derive(Clone, Debug, Deserialize, Serialize)]
struct EnrollmentGrant {
    binding: crate::features::auth::repositories::AuthenticationBinding,
    #[serde(with = "serde_bytes")]
    request_hash: Vec<u8>,
}

pub(crate) async fn issue(
    state: &AppState,
    binding: crate::features::auth::repositories::AuthenticationBinding,
) -> Result<String, ApiError> {
    let encoded = rmp_serde::to_vec_named(&EnrollmentGrant {
        binding,
        request_hash: Vec::new(),
    })
    .map_err(internal_error)?;
    for _ in 0..3 {
        let mut secret = [0_u8; 32];
        rand::rng().fill(&mut secret);
        let token = URL_SAFE_NO_PAD.encode(secret);
        if state
            .state_store
            .put_if_absent(&grant_key(&token), &encoded, ENROLLMENT_TTL)
            .await
            .map_err(internal_error)?
        {
            return Ok(token);
        }
    }
    Err(internal_error("failed to allocate device enrollment grant"))
}

/// Returns a stable enrollment capability for an idempotent authentication
/// response. The high-entropy refresh token is hashed with domain separation;
/// it is never stored in the state store.
pub(crate) async fn issue_for_refresh(
    state: &AppState,
    binding: crate::features::auth::repositories::AuthenticationBinding,
    refresh_token: &str,
) -> Result<String, ApiError> {
    let mut hasher = Sha256::new();
    hasher.update(b"kamori.device-enrollment-from-refresh.v1\0");
    hasher.update(refresh_token.as_bytes());
    let token = URL_SAFE_NO_PAD.encode(hasher.finalize());
    let encoded = rmp_serde::to_vec_named(&EnrollmentGrant {
        binding,
        request_hash: Vec::new(),
    })
    .map_err(internal_error)?;
    let key = grant_key(&token);
    if state
        .state_store
        .put_if_absent(&key, &encoded, ENROLLMENT_TTL)
        .await
        .map_err(internal_error)?
    {
        return Ok(token);
    }
    let existing = state
        .state_store
        .get(&key)
        .await
        .map_err(internal_error)?
        .ok_or_else(|| unauthenticated("device enrollment grant expired"))?;
    let existing: EnrollmentGrant = rmp_serde::from_slice(&existing).map_err(internal_error)?;
    if existing.binding != binding {
        return Err(unauthenticated("device enrollment grant is invalid"));
    }
    Ok(token)
}

pub(crate) async fn bind_request(
    state: &AppState,
    token: &str,
    user_id: Uuid,
    session_id: Uuid,
    request_bytes: &[u8],
) -> Result<crate::features::auth::repositories::AuthenticationBinding, ApiError> {
    let token = token.trim();
    if token.len() < 32 || token.len() > 256 {
        return Err(unauthenticated("valid device enrollment grant is required"));
    }
    let key = grant_key(token);
    let request_hash = Sha256::digest(request_bytes).to_vec();
    for _ in 0..3 {
        let current = state
            .state_store
            .get(&key)
            .await
            .map_err(internal_error)?
            .ok_or_else(|| unauthenticated("device enrollment grant expired"))?;
        let mut grant: EnrollmentGrant = rmp_serde::from_slice(&current).map_err(internal_error)?;
        if grant.binding.user_id != user_id || grant.binding.session_id != Some(session_id) {
            return Err(unauthenticated(
                "device enrollment grant belongs to another account",
            ));
        }
        if !grant.request_hash.is_empty() {
            return if grant.request_hash == request_hash {
                Ok(grant.binding)
            } else {
                Err(unauthenticated(
                    "device enrollment grant is already bound to another device",
                ))
            };
        }
        grant.request_hash = request_hash.clone();
        let updated = rmp_serde::to_vec_named(&grant).map_err(internal_error)?;
        if state
            .state_store
            .compare_and_set(&key, &current, &updated, ENROLLMENT_TTL)
            .await
            .map_err(internal_error)?
        {
            return Ok(grant.binding);
        }
    }
    Err(unauthenticated(
        "device enrollment grant changed concurrently; retry",
    ))
}

fn grant_key(token: &str) -> String {
    let digest = Sha256::digest(token.as_bytes());
    format!("auth:device-enrollment:{}", URL_SAFE_NO_PAD.encode(digest))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::test_support::test_state;

    #[tokio::test]
    async fn enrollment_grant_is_idempotent_only_for_the_same_device_request() {
        let state = test_state();
        let user_id = Uuid::new_v4();
        let binding = crate::features::auth::repositories::AuthenticationBinding {
            user_id,
            generation: 0,
            session_id: Some(Uuid::new_v4()),
        };
        let token = issue(&state, binding).await.expect("issue grant");

        bind_request(
            &state,
            &token,
            user_id,
            binding.session_id.unwrap(),
            b"device-a",
        )
        .await
        .expect("bind grant");
        bind_request(
            &state,
            &token,
            user_id,
            binding.session_id.unwrap(),
            b"device-a",
        )
        .await
        .expect("retry same request");
        assert!(
            bind_request(
                &state,
                &token,
                user_id,
                binding.session_id.unwrap(),
                b"device-b"
            )
            .await
            .is_err()
        );
    }

    #[tokio::test]
    async fn enrollment_grant_is_bound_to_the_authenticated_account() {
        let state = test_state();
        let binding = crate::features::auth::repositories::AuthenticationBinding {
            user_id: Uuid::new_v4(),
            generation: 0,
            session_id: Some(Uuid::new_v4()),
        };
        let token = issue(&state, binding).await.expect("issue grant");
        assert!(
            bind_request(
                &state,
                &token,
                Uuid::new_v4(),
                binding.session_id.unwrap(),
                b"device"
            )
            .await
            .is_err()
        );
    }
}
