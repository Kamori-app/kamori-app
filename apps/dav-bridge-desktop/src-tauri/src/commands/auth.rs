//! Authentication IPC commands for desktop bridge.
use crate::{
    models::{
        OpaqueSigninFinishResponse, OpaqueSigninStartResponse, PasskeyLoginFinishResponse,
        PasskeyLoginStartResponse,
    },
    state::{CollectionRecord, DesktopState},
};
use crypto_core_lib::{EncryptedGroupKey, account_keys, secret_vault};
use opaque_ke::{ClientLogin, ClientLoginFinishParameters, CredentialResponse};
use rand_core::OsRng;
use tauri::State;
use uuid::Uuid;

use super::common::{
    DesktopOpaqueSuite, MSGPACK_CONTENT_TYPE, OpaqueSigninFinishRequest, OpaqueSigninStartRequest,
    PasskeyLoginFinishRequest, PasskeyLoginStartRequest, decode_msgpack, encode_msgpack, endpoint,
    load_account_master_key_secure, load_or_create_dav_credentials, load_or_create_device_secrets,
    store_account_master_key_secure, store_refresh_token_secure, to_ui_error,
};

#[derive(serde::Serialize)]
struct RegisterDeviceRequest {
    device_id: Uuid,
    #[serde(with = "serde_bytes")]
    signing_public_key: Vec<u8>,
    #[serde(with = "serde_bytes")]
    hpke_public_key: Vec<u8>,
    #[serde(with = "serde_bytes")]
    encrypted_name: Vec<u8>,
    platform: &'static str,
}

#[derive(serde::Deserialize)]
struct DeviceKeyPackage {
    device_id: Uuid,
    key_epoch: u32,
    #[serde(with = "serde_bytes")]
    encrypted_key_package: Vec<u8>,
}

#[derive(serde::Deserialize)]
struct SpaceSummary {
    space_id: Uuid,
    key_epoch: u32,
    #[serde(with = "serde_bytes")]
    encrypted_metadata: Vec<u8>,
    device_key_packages: Vec<DeviceKeyPackage>,
}

#[derive(serde::Deserialize)]
struct ListSpacesResponse {
    spaces: Vec<SpaceSummary>,
}

#[derive(serde::Deserialize)]
struct SpaceMetadata {
    name: Option<String>,
}

#[derive(serde::Serialize)]
struct PutRecoveryKeyPackageRequest {
    key_epoch: u32,
    #[serde(with = "serde_bytes")]
    encrypted_key_package: Vec<u8>,
}

struct DesktopOpaqueLogin {
    response: OpaqueSigninFinishResponse,
    export_key: Vec<u8>,
}

async fn execute_opaque_login_round(
    base: &str,
    username: &str,
    password: &str,
    totp_code: Option<String>,
    preauth_token: Option<String>,
) -> Result<DesktopOpaqueLogin, String> {
    let mut rng = OsRng;
    let start = ClientLogin::<DesktopOpaqueSuite>::start(&mut rng, password.as_bytes())
        .map_err(|e| format!("opaque client start failed: {e:?}"))?;

    let signin_start_request = OpaqueSigninStartRequest {
        username: username.to_string(),
        opaque_start_request: start.message.serialize().to_vec(),
    };
    let signin_start_body = encode_msgpack(&signin_start_request)?;
    let signin_start_response = reqwest::Client::new()
        .post(endpoint(base, "/auth/signin/start"))
        .header(reqwest::header::CONTENT_TYPE, MSGPACK_CONTENT_TYPE)
        .header(reqwest::header::ACCEPT, MSGPACK_CONTENT_TYPE)
        .body(signin_start_body)
        .send()
        .await
        .map_err(to_ui_error)?
        .error_for_status()
        .map_err(to_ui_error)?;
    let signin_start_response: OpaqueSigninStartResponse =
        decode_msgpack(signin_start_response).await?;

    let credential_response = CredentialResponse::<DesktopOpaqueSuite>::deserialize(
        &signin_start_response.opaque_server_message,
    )
    .map_err(|e| format!("opaque server response decode failed: {e:?}"))?;

    let finish = start
        .state
        .finish(
            &mut rng,
            password.as_bytes(),
            credential_response,
            ClientLoginFinishParameters::default(),
        )
        .map_err(|e| format!("opaque client finish failed: {e:?}"))?;

    let signin_finish_request = OpaqueSigninFinishRequest {
        username: username.to_string(),
        opaque_flow_id: signin_start_response.opaque_flow_id,
        opaque_finish_request: finish.message.serialize().to_vec(),
        totp_code,
        preauth_token: preauth_token.or(signin_start_response.preauth_token),
    };
    let signin_finish_body = encode_msgpack(&signin_finish_request)?;
    let response = reqwest::Client::new()
        .post(endpoint(base, "/auth/signin/finish"))
        .header(reqwest::header::CONTENT_TYPE, MSGPACK_CONTENT_TYPE)
        .header(reqwest::header::ACCEPT, MSGPACK_CONTENT_TYPE)
        .body(signin_finish_body)
        .send()
        .await
        .map_err(to_ui_error)?
        .error_for_status()
        .map_err(to_ui_error)?;
    let response: OpaqueSigninFinishResponse = decode_msgpack(response).await?;

    Ok(DesktopOpaqueLogin {
        response,
        export_key: finish.export_key.as_slice().to_vec(),
    })
}

async fn provision_device_and_spaces(
    state: &DesktopState,
    base: &str,
    username: &str,
    access_token: &str,
    master_key: &[u8; 32],
) -> Result<(), String> {
    store_account_master_key_secure(base, username, master_key)?;
    let secrets = load_or_create_device_secrets(base, username)?;
    let encrypted_name = secret_vault::encrypt(master_key, b"Desktop").map_err(to_ui_error)?;
    let register_body = encode_msgpack(&RegisterDeviceRequest {
        device_id: secrets.device_id,
        signing_public_key: secrets.signing_public_key().to_vec(),
        hpke_public_key: secrets.hpke_public_key.to_vec(),
        encrypted_name,
        platform: "desktop",
    })?;
    reqwest::Client::new()
        .post(endpoint(base, "/devices"))
        .bearer_auth(access_token)
        .header(reqwest::header::CONTENT_TYPE, MSGPACK_CONTENT_TYPE)
        .header(reqwest::header::ACCEPT, MSGPACK_CONTENT_TYPE)
        .body(register_body)
        .send()
        .await
        .map_err(to_ui_error)?
        .error_for_status()
        .map_err(to_ui_error)?;

    let response = reqwest::Client::new()
        .get(endpoint(base, "/spaces"))
        .bearer_auth(access_token)
        .header(reqwest::header::ACCEPT, MSGPACK_CONTENT_TYPE)
        .send()
        .await
        .map_err(to_ui_error)?
        .error_for_status()
        .map_err(to_ui_error)?;
    let spaces: ListSpacesResponse = decode_msgpack(response).await?;
    let mut records = std::collections::HashMap::new();
    for space in spaces.spaces {
        let Some(package) = space.device_key_packages.iter().find(|package| {
            package.device_id == secrets.device_id && package.key_epoch == space.key_epoch
        }) else {
            continue;
        };
        let encrypted: EncryptedGroupKey =
            rmp_serde::from_slice(&package.encrypted_key_package).map_err(to_ui_error)?;
        let key = crypto_core_lib::CryptoEngine::decrypt_group_key_from_peer(
            &encrypted,
            &secrets.hpke_private_key,
        )
        .map_err(to_ui_error)?;
        let recovery_body = encode_msgpack(&PutRecoveryKeyPackageRequest {
            key_epoch: space.key_epoch,
            encrypted_key_package: secret_vault::encrypt(master_key, &key).map_err(to_ui_error)?,
        })?;
        reqwest::Client::new()
            .post(endpoint(
                base,
                &format!("/spaces/{}/recovery-key-package", space.space_id),
            ))
            .bearer_auth(access_token)
            .header(reqwest::header::CONTENT_TYPE, MSGPACK_CONTENT_TYPE)
            .header(reqwest::header::ACCEPT, MSGPACK_CONTENT_TYPE)
            .body(recovery_body)
            .send()
            .await
            .map_err(to_ui_error)?
            .error_for_status()
            .map_err(to_ui_error)?;
        let metadata = secret_vault::decrypt(&key, &space.encrypted_metadata)
            .ok()
            .and_then(|bytes| rmp_serde::from_slice::<SpaceMetadata>(&bytes).ok());
        records.insert(
            space.space_id.to_string(),
            CollectionRecord {
                id: space.space_id.to_string(),
                name: metadata
                    .and_then(|value| value.name)
                    .filter(|value| !value.trim().is_empty())
                    .unwrap_or_else(|| format!("Space {}", &space.space_id.to_string()[..8])),
                cmk: key,
                key_epoch: space.key_epoch,
                synced_items: 0,
            },
        );
    }
    *state.collections.write().await = records;
    *state.device_identity.write().await = Some(secrets.bridge_identity());
    *state.dav_credentials.write().await = Some(load_or_create_dav_credentials(base, username)?);
    state.set_username(Some(username.to_string())).await;
    Ok(())
}

/// Executes password login with OPAQUE under the hood.
///
/// UI only provides `username`, `password`, and optional `totp_code`.
#[tauri::command]
pub async fn password_login(
    state: State<'_, DesktopState>,
    username: String,
    password: String,
    totp_code: Option<String>,
) -> Result<OpaqueSigninFinishResponse, String> {
    let username = username.trim().to_string();
    if username.is_empty() {
        return Err("username is required".to_string());
    }
    if password.is_empty() {
        return Err("password is required".to_string());
    }

    let base = state.cloud_base_url().await;
    let login = execute_opaque_login_round(&base, &username, &password, totp_code, None).await?;
    let response = login.response;

    if let Some(token) = response.access_token.clone() {
        let refresh_token = response
            .refresh_token
            .clone()
            .ok_or_else(|| "missing refresh token in signin response".to_string())?;
        store_refresh_token_secure(&base, &refresh_token)?;
        let master_key = account_keys::unwrap(&login.export_key, &response.encrypted_master_key)
            .map_err(to_ui_error)?;
        provision_device_and_spaces(state.inner(), &base, &username, &token, &master_key).await?;
        state.set_access_token(Some(token)).await;
        state.set_refresh_token(Some(refresh_token)).await;
        return Ok(response);
    }

    if response.preauth_token.is_some() {
        return Err("TOTP is required. Enter TOTP code and press Login again.".to_string());
    }

    Err("Password login failed.".to_string())
}

/// Starts OPAQUE signin flow against cloud `/auth/signin/start`.
#[tauri::command]
pub async fn opaque_signin_start(
    state: State<'_, DesktopState>,
    username: String,
    opaque_start_request: Vec<u8>,
) -> Result<OpaqueSigninStartResponse, String> {
    let base = state.cloud_base_url().await;
    let url = endpoint(&base, "/auth/signin/start");

    let request = OpaqueSigninStartRequest {
        username,
        opaque_start_request,
    };

    let body = encode_msgpack(&request)?;
    let response = reqwest::Client::new()
        .post(url)
        .header(reqwest::header::CONTENT_TYPE, MSGPACK_CONTENT_TYPE)
        .header(reqwest::header::ACCEPT, MSGPACK_CONTENT_TYPE)
        .body(body)
        .send()
        .await
        .map_err(to_ui_error)?
        .error_for_status()
        .map_err(to_ui_error)?;
    decode_msgpack(response).await
}

/// Finishes OPAQUE signin and stores issued tokens when available.
#[tauri::command]
pub async fn opaque_signin_finish(
    state: State<'_, DesktopState>,
    username: String,
    opaque_flow_id: Uuid,
    opaque_finish_request: Vec<u8>,
    totp_code: Option<String>,
    preauth_token: Option<String>,
) -> Result<OpaqueSigninFinishResponse, String> {
    let base = state.cloud_base_url().await;
    let url = endpoint(&base, "/auth/signin/finish");

    let request = OpaqueSigninFinishRequest {
        username,
        opaque_flow_id,
        opaque_finish_request,
        totp_code,
        preauth_token,
    };

    let body = encode_msgpack(&request)?;
    let response = reqwest::Client::new()
        .post(url)
        .header(reqwest::header::CONTENT_TYPE, MSGPACK_CONTENT_TYPE)
        .header(reqwest::header::ACCEPT, MSGPACK_CONTENT_TYPE)
        .body(body)
        .send()
        .await
        .map_err(to_ui_error)?
        .error_for_status()
        .map_err(to_ui_error)?;
    let response: OpaqueSigninFinishResponse = decode_msgpack(response).await?;

    if let Some(token) = response.access_token.clone() {
        let refresh_token = response
            .refresh_token
            .clone()
            .ok_or_else(|| "missing refresh token in signin response".to_string())?;
        store_refresh_token_secure(&base, &refresh_token)?;
        state.set_access_token(Some(token)).await;
        state.set_refresh_token(Some(refresh_token)).await;
    }

    Ok(response)
}

/// Starts passkey login flow against cloud `/auth/passkey/login/start`.
#[tauri::command]
pub async fn passkey_login_start(
    state: State<'_, DesktopState>,
) -> Result<PasskeyLoginStartResponse, String> {
    let base = state.cloud_base_url().await;
    let url = endpoint(&base, "/auth/passkey/login/start");

    let request = PasskeyLoginStartRequest {};
    let body = encode_msgpack(&request)?;
    let response = reqwest::Client::new()
        .post(url)
        .header(reqwest::header::CONTENT_TYPE, MSGPACK_CONTENT_TYPE)
        .header(reqwest::header::ACCEPT, MSGPACK_CONTENT_TYPE)
        .body(body)
        .send()
        .await
        .map_err(to_ui_error)?
        .error_for_status()
        .map_err(to_ui_error)?;
    decode_msgpack(response).await
}

/// Finishes passkey login and stores returned access/refresh tokens.
#[tauri::command]
pub async fn passkey_login_finish(
    state: State<'_, DesktopState>,
    flow_id: String,
    credential: Vec<u8>,
) -> Result<PasskeyLoginFinishResponse, String> {
    let base = state.cloud_base_url().await;
    let url = endpoint(&base, "/auth/passkey/login/finish");

    let flow_id =
        Uuid::parse_str(flow_id.trim()).map_err(|error| format!("invalid flow_id: {error}"))?;

    let request = PasskeyLoginFinishRequest {
        flow_id,
        credential,
    };
    let body = encode_msgpack(&request)?;
    let response = reqwest::Client::new()
        .post(url)
        .header(reqwest::header::CONTENT_TYPE, MSGPACK_CONTENT_TYPE)
        .header(reqwest::header::ACCEPT, MSGPACK_CONTENT_TYPE)
        .body(body)
        .send()
        .await
        .map_err(to_ui_error)?
        .error_for_status()
        .map_err(to_ui_error)?;
    let response: PasskeyLoginFinishResponse = decode_msgpack(response).await?;
    let refresh_token = response
        .refresh_token
        .clone()
        .ok_or_else(|| "missing refresh token in passkey response".to_string())?;
    let master_key = load_account_master_key_secure(&base, &response.username)?;
    provision_device_and_spaces(
        state.inner(),
        &base,
        &response.username,
        &response.access_token,
        &master_key,
    )
    .await?;
    store_refresh_token_secure(&base, &refresh_token)?;

    state
        .set_access_token(Some(response.access_token.clone()))
        .await;
    state.set_refresh_token(Some(refresh_token)).await;
    Ok(response)
}
