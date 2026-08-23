use opaque_ke::{ClientLogin, ClientLoginFinishParameters, CredentialResponse};
use rand_core::OsRng;
use zeroize::Zeroize;

use super::MobileOpaqueSuite;
use super::{
    state::{
        MOBILE_PENDING_TOTP_LOGIN, MOBILE_REFRESH_ROTATION_REQUEST_ID, MOBILE_REFRESH_TOKEN,
        MobilePendingTotpLogin, import_mobile_refresh_credential, set_mobile_refresh_token,
    },
    transport::{
        MSGPACK_CONTENT_TYPE, decode_msgpack, encode_msgpack, endpoint, mobile_http_client,
    },
    types::{
        MobileLoginResult, MobileLogoutRequest, MobileLogoutResponse, MobileSigninFinishRequest,
        MobileSigninFinishResponse, MobileSigninStartRequest, MobileSigninStartResponse,
        MobileSigninTotpRequest,
    },
};
use crate::account_keys;

async fn execute_mobile_opaque_login_round(
    cloud_base_url: &str,
    username: &str,
    password: &str,
    totp_code: Option<String>,
) -> Result<MobileLoginResult, String> {
    let http = mobile_http_client()?;
    if let Some(code) = totp_code.as_deref().map(str::trim).filter(|code| !code.is_empty())
        && let Some(pending) = MOBILE_PENDING_TOTP_LOGIN.lock().await.clone()
    {
        if pending.username != username {
            return Err("pending TOTP continuation belongs to another account".to_string());
        }
        let response = http
            .post(endpoint(cloud_base_url, "/auth/signin/totp"))
            .header(reqwest::header::CONTENT_TYPE, MSGPACK_CONTENT_TYPE)
            .header(reqwest::header::ACCEPT, MSGPACK_CONTENT_TYPE)
            .body(encode_msgpack(&MobileSigninTotpRequest {
                continuation_token: pending.continuation_token,
                totp_code: code.to_string(),
            })?)
            .send()
            .await
            .map_err(|error| error.to_string())?
            .error_for_status()
            .map_err(|error| error.to_string())?;
        let response: MobileSigninFinishResponse = decode_msgpack(response).await?;
        return finish_mobile_login(username, response, pending.export_key).await;
    }

    let mut rng = OsRng;
    let start = ClientLogin::<MobileOpaqueSuite>::start(&mut rng, password.as_bytes())
        .map_err(|error| format!("opaque client start failed: {error:?}"))?;

    let signin_start_request = MobileSigninStartRequest {
        username: username.to_string(),
        opaque_start_request: start.message.serialize().to_vec(),
    };
    let signin_start_body = encode_msgpack(&signin_start_request)?;
    let signin_start_response = http
        .post(endpoint(cloud_base_url, "/auth/signin/start"))
        .header(reqwest::header::CONTENT_TYPE, MSGPACK_CONTENT_TYPE)
        .header(reqwest::header::ACCEPT, MSGPACK_CONTENT_TYPE)
        .body(signin_start_body)
        .send()
        .await
        .map_err(|error| error.to_string())?
        .error_for_status()
        .map_err(|error| error.to_string())?;
    let signin_start_response: MobileSigninStartResponse =
        decode_msgpack(signin_start_response).await?;

    let credential_response = CredentialResponse::<MobileOpaqueSuite>::deserialize(
        &signin_start_response.opaque_server_message,
    )
    .map_err(|error| format!("opaque server response decode failed: {error:?}"))?;

    let finish = start
        .state
        .finish(
            &mut rng,
            password.as_bytes(),
            credential_response,
            ClientLoginFinishParameters::default(),
        )
        .map_err(|error| format!("opaque client finish failed: {error:?}"))?;

    let signin_finish_request = MobileSigninFinishRequest {
        username: username.to_string(),
        opaque_flow_id: signin_start_response.opaque_flow_id,
        opaque_finish_request: finish.message.serialize().to_vec(),
        totp_code,
    };
    let signin_finish_body = encode_msgpack(&signin_finish_request)?;
    let response = http
        .post(endpoint(cloud_base_url, "/auth/signin/finish"))
        .header(reqwest::header::CONTENT_TYPE, MSGPACK_CONTENT_TYPE)
        .header(reqwest::header::ACCEPT, MSGPACK_CONTENT_TYPE)
        .body(signin_finish_body)
        .send()
        .await
        .map_err(|error| error.to_string())?
        .error_for_status()
        .map_err(|error| error.to_string())?;
    let response: MobileSigninFinishResponse = decode_msgpack(response).await?;

    if let Some(continuation_token) = response.totp_continuation_token.clone() {
        *MOBILE_PENDING_TOTP_LOGIN.lock().await = Some(MobilePendingTotpLogin {
            username: username.to_string(),
            continuation_token,
            export_key: finish.export_key.as_slice().to_vec(),
        });
    }

    finish_mobile_login(
        username,
        response,
        finish.export_key.as_slice().to_vec(),
    )
    .await
}

async fn finish_mobile_login(
    username: &str,
    response: MobileSigninFinishResponse,
    export_key: Vec<u8>,
) -> Result<MobileLoginResult, String> {
    set_mobile_refresh_token(response.refresh_token.clone()).await;

    let account_master_key = if response.access_token.is_some() {
        Some(
            account_keys::unwrap(&export_key, &response.encrypted_master_key)
                .map_err(|error| format!("failed to unlock account master key: {error}"))?,
        )
    } else {
        None
    };

    if response.access_token.is_some() {
        *MOBILE_PENDING_TOTP_LOGIN.lock().await = None;
    }

    Ok(MobileLoginResult {
        username: response
            .access_token
            .as_ref()
            .map(|_| username.to_string()),
        access_token: response.access_token,
        totp_continuation_token: response.totp_continuation_token,
        device_enrollment_token: response.device_enrollment_token,
        totp_verified: response.totp_verified,
        account_master_key,
    })
}

pub(super) async fn mobile_password_login_impl(
    cloud_base_url: String,
    username: String,
    password: String,
    totp_code: Option<String>,
) -> Result<MobileLoginResult, String> {
    let cloud_base_url = crate::local_bridge_runner::normalize_cloud_base_url(&cloud_base_url)
        .map_err(|error| error.to_string())?;
    let username = username.trim().to_string();
    if username.is_empty() {
        return Err("username is required".to_string());
    }
    if password.is_empty() {
        return Err("password is required".to_string());
    }

    execute_mobile_opaque_login_round(&cloud_base_url, &username, &password, totp_code).await
}

pub(super) async fn mobile_import_refresh_token_impl(
    refresh_token: String,
    rotation_request_id: String,
) -> Result<(), String> {
    let refresh_token = refresh_token.trim().to_string();
    if refresh_token.is_empty() {
        return Err("refresh token is required".to_string());
    }
    let rotation_request_id = uuid::Uuid::parse_str(rotation_request_id.trim())
        .map_err(|error| format!("invalid refresh rotation request id: {error}"))?;
    import_mobile_refresh_credential(refresh_token, rotation_request_id).await;
    Ok(())
}

pub(super) async fn mobile_export_refresh_token_impl() -> Option<String> {
    MOBILE_REFRESH_TOKEN.lock().await.clone()
}

pub(super) async fn mobile_export_refresh_rotation_request_id_impl() -> Option<String> {
    MOBILE_REFRESH_ROTATION_REQUEST_ID
        .lock()
        .await
        .map(|value| value.to_string())
}

pub(super) async fn mobile_clear_refresh_token_impl() {
    let _lease = super::state::MOBILE_RUNTIME_LEASE.lock().await;
    set_mobile_refresh_token(None).await;
    if let Some(mut key) = super::state::MOBILE_ACCOUNT_MASTER_KEY.lock().await.take() {
        key.fill(0);
    }
    let mut collection_keys = super::state::MOBILE_COLLECTION_KEYS.lock().await;
    for (_, (_, mut key)) in collection_keys.drain() {
        key.zeroize();
    }
    drop(collection_keys);
    super::state::MOBILE_SYNC_STARTS.lock().await.clear();
    if let Some(mut device) = super::state::MOBILE_DEVICE_SECRETS.lock().await.take() {
        device.device_id.zeroize();
        device.signing_private_key.zeroize();
        device.hpke_private_key.zeroize();
        device.hpke_public_key.zeroize();
    }
    if let Some(mut pending) = MOBILE_PENDING_TOTP_LOGIN.lock().await.take() {
        pending.username.zeroize();
        pending.continuation_token.zeroize();
        pending.export_key.zeroize();
    }
    if let Err(error) = super::state::clear_mobile_runtime().await {
        tracing::warn!(%error, "failed to clear persisted mobile runtime credentials");
    }
}

pub(super) async fn mobile_revoke_refresh_session_impl(
    cloud_base_url: String,
    refresh_token: String,
) -> Result<bool, String> {
    let cloud_base_url = crate::local_bridge_runner::normalize_cloud_base_url(&cloud_base_url)
        .map_err(|error| error.to_string())?;
    let refresh_token = refresh_token.trim().to_owned();
    if refresh_token.is_empty() {
        return Err("refresh token is required".to_string());
    }
    let response = mobile_http_client()?
        .post(endpoint(&cloud_base_url, "/auth/logout"))
        .header(reqwest::header::CONTENT_TYPE, MSGPACK_CONTENT_TYPE)
        .header(reqwest::header::ACCEPT, MSGPACK_CONTENT_TYPE)
        .body(encode_msgpack(&MobileLogoutRequest {
            refresh_token: Some(refresh_token),
        })?)
        .send()
        .await
        .map_err(|error| error.to_string())?
        .error_for_status()
        .map_err(|error| error.to_string())?;
    Ok(decode_msgpack::<MobileLogoutResponse>(response).await?.revoked)
}
