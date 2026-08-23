//! Authentication IPC commands for desktop bridge.
use crate::{
    models::{
        BrowserLoginPollResponse, BrowserLoginStartResponse, OpaqueSigninFinishResponse,
        OpaqueSigninStartResponse,
    },
    state::{CollectionRecord, DesktopState, PendingBrowserLogin, PendingTotpLogin},
};
use crypto_core_lib::{
    CryptoEngine, EncryptedDeviceBootstrap, EncryptedGroupKey, account_keys,
    local_bridge_runner::LocalBridgeRunner, secret_vault,
};
use opaque_ke::{ClientLogin, ClientLoginFinishParameters, CredentialResponse};
use rand_core::OsRng;
use tauri::{AppHandle, State};
use tauri_plugin_opener::OpenerExt;
use uuid::Uuid;
use zeroize::Zeroizing;

use super::common::{
    BrowserLoginPollRequest, BrowserLoginStartRequest, DesktopOpaqueSuite, MSGPACK_CONTENT_TYPE,
    OpaqueSigninFinishRequest, OpaqueSigninStartRequest, SigninTotpRequest,
    clear_refresh_token_secure, clear_session_username_secure, decode_msgpack, encode_msgpack,
    endpoint, load_account_master_key_secure, load_or_create_dav_credentials,
    load_or_create_device_secrets, load_refresh_credential_secure, load_session_username_secure,
    revoke_refresh_session, store_account_master_key_secure, store_refresh_token_secure,
    store_session_username_secure, to_ui_error,
};

#[derive(serde::Serialize)]
struct RegisterDeviceRequest {
    enrollment_token: String,
    device_id: Uuid,
    #[serde(with = "serde_bytes")]
    signing_public_key: Vec<u8>,
    #[serde(with = "serde_bytes")]
    hpke_public_key: Vec<u8>,
    #[serde(with = "serde_bytes")]
    encrypted_name: Vec<u8>,
    platform: &'static str,
}

#[derive(serde::Serialize)]
struct RefreshRequest {
    refresh_token: Option<String>,
    rotation_request_id: Uuid,
}

#[derive(serde::Deserialize)]
struct RefreshResponse {
    access_token: String,
    username: String,
    refresh_token: Option<String>,
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
    history_start_seq: u64,
    current_state_start_seq: u64,
    #[serde(with = "serde_bytes")]
    encrypted_metadata: Vec<u8>,
    device_key_packages: Vec<DeviceKeyPackage>,
}

#[derive(serde::Deserialize)]
struct ListSpacesResponse {
    spaces: Vec<SpaceSummary>,
}

#[derive(serde::Deserialize)]
struct RecoverySpaceKeyPackage {
    space_id: Uuid,
    key_epoch: u32,
    #[serde(with = "serde_bytes")]
    encrypted_key_package: Vec<u8>,
}

#[derive(serde::Deserialize)]
struct ListRecoveryKeyPackagesResponse {
    packages: Vec<RecoverySpaceKeyPackage>,
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

#[derive(serde::Serialize)]
struct PutDeviceKeyPackageRequest {
    package: PutDeviceKeyPackage,
}

#[derive(serde::Serialize)]
struct PutDeviceKeyPackage {
    device_id: Uuid,
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

async fn execute_totp_continuation(
    base: &str,
    continuation_token: String,
    totp_code: String,
) -> Result<OpaqueSigninFinishResponse, String> {
    let response = reqwest::Client::new()
        .post(endpoint(base, "/auth/signin/totp"))
        .header(reqwest::header::CONTENT_TYPE, MSGPACK_CONTENT_TYPE)
        .header(reqwest::header::ACCEPT, MSGPACK_CONTENT_TYPE)
        .body(encode_msgpack(&SigninTotpRequest {
            continuation_token,
            totp_code,
        })?)
        .send()
        .await
        .map_err(to_ui_error)?
        .error_for_status()
        .map_err(to_ui_error)?;
    decode_msgpack(response).await
}

async fn authenticated_get_msgpack<T: serde::de::DeserializeOwned>(
    runner: Option<&LocalBridgeRunner>,
    base: &str,
    access_token: &str,
    path: &str,
) -> Result<T, String> {
    if let Some(runner) = runner {
        return rmp_serde::from_slice(&runner.cloud_get_msgpack(path).await.map_err(to_ui_error)?)
            .map_err(to_ui_error);
    }
    let response = reqwest::Client::new()
        .get(endpoint(base, path))
        .bearer_auth(access_token)
        .header(reqwest::header::ACCEPT, MSGPACK_CONTENT_TYPE)
        .send()
        .await
        .map_err(to_ui_error)?
        .error_for_status()
        .map_err(to_ui_error)?;
    decode_msgpack(response).await
}

async fn authenticated_post_msgpack(
    runner: Option<&LocalBridgeRunner>,
    base: &str,
    access_token: &str,
    path: &str,
    body: Vec<u8>,
) -> Result<(), String> {
    if let Some(runner) = runner {
        runner
            .cloud_post_msgpack(path, body)
            .await
            .map_err(to_ui_error)?;
        return Ok(());
    }
    reqwest::Client::new()
        .post(endpoint(base, path))
        .bearer_auth(access_token)
        .header(reqwest::header::CONTENT_TYPE, MSGPACK_CONTENT_TYPE)
        .header(reqwest::header::ACCEPT, MSGPACK_CONTENT_TYPE)
        .body(body)
        .send()
        .await
        .map_err(to_ui_error)?
        .error_for_status()
        .map_err(to_ui_error)?;
    Ok(())
}

async fn provision_device_and_spaces(
    state: &DesktopState,
    base: &str,
    username: &str,
    access_token: &str,
    enrollment_token: Option<&str>,
    master_key: &[u8; 32],
    runner: Option<&LocalBridgeRunner>,
) -> Result<(), String> {
    store_account_master_key_secure(base, username, master_key)?;
    let secrets = load_or_create_device_secrets(base, username)?;
    let encrypted_name = secret_vault::encrypt(master_key, b"Desktop").map_err(to_ui_error)?;
    if let Some(enrollment_token) = enrollment_token {
        let register_body = encode_msgpack(&RegisterDeviceRequest {
            enrollment_token: enrollment_token.to_string(),
            device_id: secrets.device_id,
            signing_public_key: secrets.signing_public_key().to_vec(),
            hpke_public_key: secrets.hpke_public_key.to_vec(),
            encrypted_name,
            platform: "desktop",
        })?;
        authenticated_post_msgpack(runner, base, access_token, "/devices", register_body).await?;
    }

    let spaces: ListSpacesResponse =
        authenticated_get_msgpack(runner, base, access_token, "/spaces").await?;
    let recovery_packages: ListRecoveryKeyPackagesResponse =
        authenticated_get_msgpack(runner, base, access_token, "/spaces/recovery-key-packages")
            .await?;
    let recovery_packages = recovery_packages
        .packages
        .into_iter()
        .map(|package| {
            (
                (package.space_id, package.key_epoch),
                package.encrypted_key_package,
            )
        })
        .collect::<std::collections::HashMap<_, _>>();
    let mut records = std::collections::HashMap::new();
    for space in spaces.spaces {
        let device_package = space.device_key_packages.iter().find(|package| {
            package.device_id == secrets.device_id && package.key_epoch == space.key_epoch
        });
        let key = if let Some(package) = device_package {
            let encrypted: EncryptedGroupKey =
                rmp_serde::from_slice(&package.encrypted_key_package).map_err(to_ui_error)?;
            crypto_core_lib::CryptoEngine::decrypt_group_key_from_peer(
                &encrypted,
                &secrets.hpke_private_key,
            )
            .map_err(to_ui_error)?
        } else if let Some(package) = recovery_packages.get(&(space.space_id, space.key_epoch)) {
            let encrypted: EncryptedGroupKey =
                rmp_serde::from_slice(package).map_err(to_ui_error)?;
            let recovery_identity =
                crypto_core_lib::CryptoEngine::derive_account_recovery_keypair(master_key);
            crypto_core_lib::CryptoEngine::decrypt_group_key_from_peer(
                &encrypted,
                &recovery_identity.private_key,
            )
            .map_err(to_ui_error)?
        } else {
            continue;
        };
        if device_package.is_none() {
            let encrypted = crypto_core_lib::CryptoEngine::encrypt_group_key_for_peer(
                &key,
                &secrets.hpke_public_key,
            )
            .map_err(to_ui_error)?;
            let body = encode_msgpack(&PutDeviceKeyPackageRequest {
                package: PutDeviceKeyPackage {
                    device_id: secrets.device_id,
                    key_epoch: space.key_epoch,
                    encrypted_key_package: rmp_serde::to_vec_named(&encrypted)
                        .map_err(to_ui_error)?,
                },
            })?;
            authenticated_post_msgpack(
                runner,
                base,
                access_token,
                &format!("/spaces/{}/device-key-packages", space.space_id),
                body,
            )
            .await?;
        }
        let recovery_body = encode_msgpack(&PutRecoveryKeyPackageRequest {
            key_epoch: space.key_epoch,
            encrypted_key_package: rmp_serde::to_vec_named(
                &crypto_core_lib::CryptoEngine::encrypt_group_key_for_peer(
                    &key,
                    &crypto_core_lib::CryptoEngine::derive_account_recovery_keypair(master_key)
                        .public_key,
                )
                .map_err(to_ui_error)?,
            )
            .map_err(to_ui_error)?,
        })?;
        authenticated_post_msgpack(
            runner,
            base,
            access_token,
            &format!("/spaces/{}/recovery-key-package", space.space_id),
            recovery_body,
        )
        .await?;
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
                sync_start_seq: space.history_start_seq.max(space.current_state_start_seq),
                synced_items: 0,
            },
        );
    }
    *state.collections.write().await = records;
    *state.device_identity.write().await = Some(secrets.bridge_identity());
    *state.dav_credentials.write().await = Some(load_or_create_dav_credentials(base, username)?);
    store_session_username_secure(base, username)?;
    state.set_username(Some(username.to_string())).await;
    Ok(())
}

pub(super) async fn reconcile_device_and_spaces(
    state: &DesktopState,
    runner: &LocalBridgeRunner,
) -> Result<(), String> {
    let base = state.cloud_base_url().await;
    let username = state
        .username()
        .await
        .ok_or_else(|| "sign in again to refresh security-space keys".to_string())?;
    let master_key = load_account_master_key_secure(&base, &username)?;
    let previous_ids = state
        .collections
        .read()
        .await
        .keys()
        .cloned()
        .collect::<std::collections::HashSet<_>>();
    let access_token = runner.current_access_token().await;
    provision_device_and_spaces(
        state,
        &base,
        &username,
        &access_token,
        None,
        &master_key,
        Some(runner),
    )
    .await?;
    let current = state.collections.read().await.clone();
    let current_ids = current
        .keys()
        .cloned()
        .collect::<std::collections::HashSet<_>>();
    for removed in previous_ids.difference(&current_ids) {
        let _ = runner.unregister_collection_key(removed).await;
    }
    for collection in current.values() {
        runner
            .register_collection_key_epoch_from(
                collection.id.clone(),
                collection.key_epoch,
                collection.cmk,
                collection.sync_start_seq,
            )
            .await
            .map_err(to_ui_error)?;
    }
    state
        .set_access_token(Some(runner.current_access_token().await))
        .await;
    Ok(())
}

/// Restores a keychain-backed desktop session after process restart.
#[tauri::command]
pub async fn restore_session(state: State<'_, DesktopState>) -> Result<bool, String> {
    let base = state.cloud_base_url().await;
    let refresh_credential = load_refresh_credential_secure(&base)?;
    let stored_username = load_session_username_secure(&base)?;
    let (Some((refresh_token, rotation_request_id)), Some(_stored_username)) =
        (refresh_credential, stored_username)
    else {
        // Remove an orphan half-session so later launches are deterministic.
        clear_refresh_token_secure(&base)?;
        clear_session_username_secure(&base)?;
        return Ok(false);
    };
    let response = reqwest::Client::new()
        .post(endpoint(&base, "/auth/refresh"))
        .header(reqwest::header::CONTENT_TYPE, MSGPACK_CONTENT_TYPE)
        .header(reqwest::header::ACCEPT, MSGPACK_CONTENT_TYPE)
        .body(encode_msgpack(&RefreshRequest {
            refresh_token: Some(refresh_token.clone()),
            rotation_request_id,
        })?)
        .send()
        .await
        .map_err(to_ui_error)?;
    if response.status() == reqwest::StatusCode::UNAUTHORIZED {
        clear_refresh_token_secure(&base)?;
        clear_session_username_secure(&base)?;
        return Ok(false);
    }
    let response = response.error_for_status().map_err(to_ui_error)?;
    let refreshed: RefreshResponse = decode_msgpack(response).await?;
    let rotated_refresh = refreshed
        .refresh_token
        .filter(|token| !token.trim().is_empty())
        .ok_or_else(|| "refresh response did not contain a refresh token".to_string())?;
    let username = refreshed.username.trim().to_string();
    if username.is_empty() {
        let _ = revoke_refresh_session(&base, &rotated_refresh).await;
        clear_refresh_token_secure(&base)?;
        clear_session_username_secure(&base)?;
        return Err("refresh response did not contain an account identity".to_string());
    }
    let master_key = match load_account_master_key_secure(&base, &username) {
        Ok(key) => key,
        Err(error) => {
            let _ = revoke_refresh_session(&base, &rotated_refresh).await;
            clear_refresh_token_secure(&base)?;
            clear_session_username_secure(&base)?;
            return Err(error);
        }
    };
    if let Err(error) = provision_device_and_spaces(
        state.inner(),
        &base,
        &username,
        &refreshed.access_token,
        None,
        &master_key,
        None,
    )
    .await
    {
        let _ = revoke_refresh_session(&base, &rotated_refresh).await;
        clear_refresh_token_secure(&base)?;
        clear_session_username_secure(&base)?;
        return Err(error);
    }
    if let Err(error) = store_refresh_token_secure(&base, &rotated_refresh) {
        let _ = revoke_refresh_session(&base, &rotated_refresh).await;
        clear_refresh_token_secure(&base)?;
        clear_session_username_secure(&base)?;
        return Err(error);
    }
    state.set_access_token(Some(refreshed.access_token)).await;
    state.set_refresh_token(Some(rotated_refresh)).await;
    Ok(true)
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
    let pending = state.pending_totp_login.lock().await.clone();
    let login = if let (Some(pending), Some(code)) = (pending, totp_code.clone()) {
        if pending.username != username {
            return Err("The pending TOTP continuation belongs to another account.".to_string());
        }
        let response = execute_totp_continuation(&base, pending.continuation_token, code).await?;
        DesktopOpaqueLogin {
            response,
            export_key: pending.export_key,
        }
    } else {
        execute_opaque_login_round(&base, &username, &password, totp_code).await?
    };
    let response = login.response;

    if let Some(token) = response.access_token.clone() {
        let refresh_token = response
            .refresh_token
            .clone()
            .ok_or_else(|| "missing refresh token in signin response".to_string())?;
        let master_key = account_keys::unwrap(&login.export_key, &response.encrypted_master_key)
            .map_err(to_ui_error)?;
        let enrollment_token = response
            .device_enrollment_token
            .as_deref()
            .ok_or_else(|| "missing device enrollment capability".to_string())?;
        if let Err(error) = provision_device_and_spaces(
            state.inner(),
            &base,
            &username,
            &token,
            Some(enrollment_token),
            &master_key,
            None,
        )
        .await
        {
            let cleanup = revoke_refresh_session(&base, &refresh_token).await;
            return Err(match cleanup {
                Ok(_) => error,
                Err(cleanup_error) => format!(
                    "{error}; the incomplete server session could not be revoked: {cleanup_error}"
                ),
            });
        }
        if let Err(error) = store_refresh_token_secure(&base, &refresh_token) {
            let cleanup = revoke_refresh_session(&base, &refresh_token).await;
            return Err(match cleanup {
                Ok(_) => error,
                Err(cleanup_error) => format!(
                    "{error}; the server session created during login could not be revoked: {cleanup_error}"
                ),
            });
        }
        state.set_access_token(Some(token)).await;
        state.set_refresh_token(Some(refresh_token)).await;
        *state.pending_totp_login.lock().await = None;
        return Ok(response);
    }

    if let Some(continuation_token) = response.totp_continuation_token.clone() {
        *state.pending_totp_login.lock().await = Some(PendingTotpLogin {
            username,
            continuation_token,
            export_key: login.export_key,
        });
        return Err("TOTP is required. Enter TOTP code and press Login again.".to_string());
    }

    Err("Password login failed.".to_string())
}

/// Starts device authorization and opens the trusted web origin in the system browser.
#[tauri::command]
pub async fn browser_login_start(
    app: AppHandle,
    state: State<'_, DesktopState>,
) -> Result<BrowserLoginStartResponse, String> {
    let base = state.cloud_base_url().await;
    let url = endpoint(&base, "/auth/device-authorization/start");

    let bootstrap_identity = CryptoEngine::generate_x25519_keypair();
    let request = BrowserLoginStartRequest {
        hpke_public_key: bootstrap_identity.public_key.to_vec(),
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
    let response: BrowserLoginStartResponse = decode_msgpack(response).await?;
    *state.pending_browser_login.lock().await = Some(PendingBrowserLogin {
        flow_id: response.flow_id,
        hpke_private_key: Zeroizing::new(bootstrap_identity.private_key),
    });
    if let Err(error) = app
        .opener()
        .open_url(&response.verification_uri, None::<&str>)
    {
        state.pending_browser_login.lock().await.take();
        return Err(to_ui_error(error));
    }
    Ok(response)
}

/// Polls one external-browser authorization without exposing tokens to the browser URL.
#[tauri::command]
pub async fn browser_login_poll(
    state: State<'_, DesktopState>,
    flow_id: String,
    device_secret: String,
) -> Result<BrowserLoginPollResponse, String> {
    let base = state.cloud_base_url().await;
    let url = endpoint(&base, "/auth/device-authorization/token");

    let flow_id =
        Uuid::parse_str(flow_id.trim()).map_err(|error| format!("invalid flow_id: {error}"))?;
    {
        let pending = state.pending_browser_login.lock().await;
        if pending.as_ref().map(|value| value.flow_id) != Some(flow_id) {
            return Err("browser authorization does not belong to this app session".to_string());
        }
    }

    let request = BrowserLoginPollRequest {
        flow_id,
        device_secret,
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
    let response: BrowserLoginPollResponse = decode_msgpack(response).await?;
    if response.status == "pending" {
        return Ok(response);
    }
    let pending = state
        .pending_browser_login
        .lock()
        .await
        .take()
        .ok_or_else(|| "browser authorization key is no longer available".to_string())?;
    let username = response
        .username
        .as_deref()
        .ok_or_else(|| "browser authorization returned no username".to_string())?;
    let access_token = response
        .access_token
        .as_deref()
        .ok_or_else(|| "browser authorization returned no access token".to_string())?;
    let refresh_token = response
        .refresh_token
        .clone()
        .ok_or_else(|| "browser authorization returned no refresh token".to_string())?;
    let enrollment_token = response.device_enrollment_token.as_deref().ok_or_else(|| {
        "browser authorization returned no device enrollment capability".to_string()
    })?;
    let encrypted_master_key: EncryptedDeviceBootstrap = match rmp_serde::from_slice(
        &response.encrypted_master_key_package,
    ) {
        Ok(package) => package,
        Err(_) => {
            let cleanup = revoke_refresh_session(&base, &refresh_token).await;
            return Err(match cleanup {
                Ok(_) => "browser authorization returned an invalid encrypted key".to_string(),
                Err(cleanup_error) => format!(
                    "browser authorization returned an invalid encrypted key; the incomplete server session could not be revoked: {cleanup_error}"
                ),
            });
        }
    };
    let master_key = match CryptoEngine::decrypt_device_bootstrap(
        &encrypted_master_key,
        &pending.hpke_private_key,
        flow_id,
    ) {
        Ok(master_key) => master_key,
        Err(error) => {
            let cleanup = revoke_refresh_session(&base, &refresh_token).await;
            return Err(match cleanup {
                Ok(_) => format!("browser authorization key could not be decrypted: {error}"),
                Err(cleanup_error) => format!(
                    "browser authorization key could not be decrypted: {error}; the incomplete server session could not be revoked: {cleanup_error}"
                ),
            });
        }
    };
    if let Err(error) = provision_device_and_spaces(
        state.inner(),
        &base,
        username,
        access_token,
        Some(enrollment_token),
        &master_key,
        None,
    )
    .await
    {
        let cleanup = revoke_refresh_session(&base, &refresh_token).await;
        return Err(match cleanup {
            Ok(_) => error,
            Err(cleanup_error) => format!(
                "{error}; the incomplete server session could not be revoked: {cleanup_error}"
            ),
        });
    }
    if let Err(error) = store_refresh_token_secure(&base, &refresh_token) {
        let cleanup = revoke_refresh_session(&base, &refresh_token).await;
        return Err(match cleanup {
            Ok(_) => error,
            Err(cleanup_error) => format!(
                "{error}; the browser-authorized server session could not be revoked: {cleanup_error}"
            ),
        });
    }

    state.set_access_token(Some(access_token.to_string())).await;
    state.set_username(Some(username.to_string())).await;
    state.set_refresh_token(Some(refresh_token)).await;
    Ok(response)
}
