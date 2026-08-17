//! External-browser authorization for native desktop clients.

use axum::http::HeaderMap;
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use rand::RngExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::time::Duration;
use time::OffsetDateTime;
use url::Url;
use uuid::Uuid;

use crate::{
    features::auth::{
        dto::{
            DeviceAuthorizationApproveRequest, DeviceAuthorizationApproveResponse,
            DeviceAuthorizationStartResponse, DeviceAuthorizationStatus,
            DeviceAuthorizationTokenRequest, DeviceAuthorizationTokenResponse,
        },
        repositories::{create_refresh_token, find_active_username_by_id},
        transport::client_metadata_from_headers,
    },
    features::common::{ApiError, authorize_session, bad_request, internal_error, unauthenticated},
    platform::state::AppState,
};

const FLOW_TTL: Duration = Duration::from_secs(10 * 60);
const POLL_INTERVAL_SECONDS: u64 = 2;

#[derive(Debug, Serialize, Deserialize)]
struct DeviceFlow {
    secret_hash: Vec<u8>,
    user_code: String,
    approved_user_id: Option<Uuid>,
    approved_username: Option<String>,
}

fn flow_key(flow_id: Uuid) -> String {
    format!("auth:device-flow:{flow_id}")
}

fn code_key(code: &str) -> String {
    format!("auth:device-code:{code}")
}

fn normalize_code(code: &str) -> Result<String, ApiError> {
    let normalized = code
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .map(|character| character.to_ascii_uppercase())
        .collect::<String>();
    if normalized.len() != 10 {
        return Err(bad_request("device code must contain 10 characters"));
    }
    Ok(normalized)
}

fn secret_matches(expected: &[u8], provided: &str) -> bool {
    let actual = Sha256::digest(provided.as_bytes());
    expected.len() == actual.len()
        && expected
            .iter()
            .zip(actual.iter())
            .fold(0_u8, |difference, (left, right)| {
                difference | (left ^ right)
            })
            == 0
}

fn verification_uri(state: &AppState, user_code: &str) -> Result<String, ApiError> {
    let mut url = Url::parse(&state.config.webauthn_rp_origin).map_err(internal_error)?;
    url.set_path("/app");
    url.set_query(None);
    url.query_pairs_mut().append_pair("device_code", user_code);
    Ok(url.to_string())
}

pub(crate) async fn start(state: &AppState) -> Result<DeviceAuthorizationStartResponse, ApiError> {
    let flow_id = Uuid::new_v4();
    let mut secret = [0_u8; 32];
    rand::rng().fill(&mut secret);
    let device_secret = URL_SAFE_NO_PAD.encode(secret);
    let user_code = loop {
        let candidate = Uuid::new_v4().simple().to_string()[..10].to_ascii_uppercase();
        let occupied = state
            .state_store
            .get(&code_key(&candidate))
            .await
            .map_err(internal_error)?
            .is_some();
        if !occupied {
            break candidate;
        }
    };
    let flow = DeviceFlow {
        secret_hash: Sha256::digest(device_secret.as_bytes()).to_vec(),
        user_code: user_code.clone(),
        approved_user_id: None,
        approved_username: None,
    };
    state
        .state_store
        .put(
            &flow_key(flow_id),
            &rmp_serde::to_vec_named(&flow).map_err(internal_error)?,
            FLOW_TTL,
        )
        .await
        .map_err(internal_error)?;
    state
        .state_store
        .put(&code_key(&user_code), flow_id.as_bytes(), FLOW_TTL)
        .await
        .map_err(internal_error)?;

    Ok(DeviceAuthorizationStartResponse {
        flow_id,
        device_secret,
        user_code: user_code.clone(),
        verification_uri: verification_uri(state, &user_code)?,
        expires_in_seconds: FLOW_TTL.as_secs(),
        poll_interval_seconds: POLL_INTERVAL_SECONDS,
    })
}

pub(crate) async fn approve(
    state: &AppState,
    headers: &HeaderMap,
    request: DeviceAuthorizationApproveRequest,
) -> Result<DeviceAuthorizationApproveResponse, ApiError> {
    let user_id = authorize_session(state, headers).await?;
    let username = find_active_username_by_id(&state.pool, user_id)
        .await?
        .ok_or_else(|| unauthenticated("account is unavailable"))?;
    let user_code = normalize_code(&request.user_code)?;
    let flow_id_bytes = state
        .state_store
        .get(&code_key(&user_code))
        .await
        .map_err(internal_error)?
        .ok_or_else(|| bad_request("device authorization has expired"))?;
    let flow_id = Uuid::from_slice(&flow_id_bytes)
        .map_err(|_| internal_error("stored device authorization is invalid"))?;
    let flow_bytes = state
        .state_store
        .get(&flow_key(flow_id))
        .await
        .map_err(internal_error)?
        .ok_or_else(|| bad_request("device authorization has expired"))?;
    let mut flow: DeviceFlow = rmp_serde::from_slice(&flow_bytes).map_err(internal_error)?;
    if flow.user_code != user_code {
        return Err(bad_request("device authorization is invalid"));
    }
    if flow.approved_user_id.is_some() && flow.approved_user_id != Some(user_id) {
        return Err(bad_request("device authorization was already approved"));
    }
    flow.approved_user_id = Some(user_id);
    flow.approved_username = Some(username);
    state
        .state_store
        .put(
            &flow_key(flow_id),
            &rmp_serde::to_vec_named(&flow).map_err(internal_error)?,
            FLOW_TTL,
        )
        .await
        .map_err(internal_error)?;
    Ok(DeviceAuthorizationApproveResponse { approved: true })
}

pub(crate) async fn token(
    state: &AppState,
    headers: &HeaderMap,
    request: DeviceAuthorizationTokenRequest,
) -> Result<DeviceAuthorizationTokenResponse, ApiError> {
    let flow_bytes = state
        .state_store
        .get(&flow_key(request.flow_id))
        .await
        .map_err(internal_error)?
        .ok_or_else(|| unauthenticated("device authorization has expired"))?;
    let flow: DeviceFlow = rmp_serde::from_slice(&flow_bytes).map_err(internal_error)?;
    if !secret_matches(&flow.secret_hash, &request.device_secret) {
        return Err(unauthenticated("invalid device authorization"));
    }
    let (Some(_), Some(_)) = (flow.approved_user_id, flow.approved_username.as_ref()) else {
        return Ok(DeviceAuthorizationTokenResponse {
            status: DeviceAuthorizationStatus::Pending,
            username: None,
            access_token: None,
            refresh_token: None,
            refresh_token_id: None,
        });
    };

    let claimed = state
        .state_store
        .take(&flow_key(request.flow_id))
        .await
        .map_err(internal_error)?
        .ok_or_else(|| unauthenticated("device authorization was already consumed"))?;
    let claimed: DeviceFlow = rmp_serde::from_slice(&claimed).map_err(internal_error)?;
    if !secret_matches(&claimed.secret_hash, &request.device_secret) {
        return Err(unauthenticated("invalid device authorization"));
    }
    let (Some(user_id), Some(username)) = (claimed.approved_user_id, claimed.approved_username)
    else {
        return Err(unauthenticated("device authorization is not approved"));
    };

    let client = client_metadata_from_headers(headers);
    let refresh = create_refresh_token(
        &state.pool,
        user_id,
        client.user_agent.as_deref(),
        client.ip_address.as_deref(),
        OffsetDateTime::now_utc() + state.refresh_token_ttl(),
    )
    .await?;
    let access_token = state
        .issue_access_token(user_id, &username)
        .map_err(internal_error)?;
    state
        .state_store
        .delete(&code_key(&claimed.user_code))
        .await
        .map_err(internal_error)?;

    Ok(DeviceAuthorizationTokenResponse {
        status: DeviceAuthorizationStatus::Approved,
        username: Some(username),
        access_token: Some(access_token),
        refresh_token: Some(refresh.token),
        refresh_token_id: Some(refresh.token_id),
    })
}
