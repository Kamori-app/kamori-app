use opaque_ke::{ClientLogin, ClientLoginFinishParameters, CredentialResponse};
use rand_core::OsRng;

use super::MobileOpaqueSuite;
use super::{
    state::{MOBILE_REFRESH_TOKEN, set_mobile_refresh_token},
    transport::{
        MSGPACK_CONTENT_TYPE, decode_msgpack, encode_msgpack, endpoint, mobile_http_client,
    },
    types::{
        MobileLoginResult, MobileSigninFinishRequest, MobileSigninFinishResponse,
        MobileSigninStartRequest, MobileSigninStartResponse, MobileLogoutRequest,
        MobileLogoutResponse,
    },
};
use crate::account_keys;

async fn execute_mobile_opaque_login_round(
    cloud_base_url: &str,
    username: &str,
    password: &str,
    totp_code: Option<String>,
) -> Result<MobileLoginResult, String> {
    let mut rng = OsRng;
    let http = mobile_http_client()?;
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
        preauth_token: signin_start_response.preauth_token,
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

    set_mobile_refresh_token(response.refresh_token.clone()).await;

    let account_master_key = if response.access_token.is_some() {
        Some(
            account_keys::unwrap(
                finish.export_key.as_slice(),
                &response.encrypted_master_key,
            )
            .map_err(|error| format!("failed to unlock account master key: {error}"))?,
        )
    } else {
        None
    };

    Ok(MobileLoginResult {
        username: response
            .access_token
            .as_ref()
            .map(|_| username.to_string()),
        access_token: response.access_token,
        preauth_token: response.preauth_token,
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
    let username = username.trim().to_string();
    if username.is_empty() {
        return Err("username is required".to_string());
    }
    if password.is_empty() {
        return Err("password is required".to_string());
    }

    execute_mobile_opaque_login_round(&cloud_base_url, &username, &password, totp_code).await
}

pub(super) async fn mobile_import_refresh_token_impl(refresh_token: String) -> Result<(), String> {
    let refresh_token = refresh_token.trim().to_string();
    if refresh_token.is_empty() {
        return Err("refresh token is required".to_string());
    }
    set_mobile_refresh_token(Some(refresh_token)).await;
    Ok(())
}

pub(super) async fn mobile_export_refresh_token_impl() -> Option<String> {
    MOBILE_REFRESH_TOKEN.lock().await.clone()
}

pub(super) async fn mobile_clear_refresh_token_impl() {
    set_mobile_refresh_token(None).await;
    if let Some(mut key) = super::state::MOBILE_ACCOUNT_MASTER_KEY.lock().await.take() {
        key.fill(0);
    }
    super::state::MOBILE_COLLECTION_KEYS.lock().await.clear();
    *super::state::MOBILE_DEVICE_SECRETS.lock().await = None;
}

pub(super) async fn mobile_revoke_refresh_session_impl(
    cloud_base_url: String,
    refresh_token: String,
) -> Result<bool, String> {
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
