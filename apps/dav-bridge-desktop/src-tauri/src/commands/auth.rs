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
use tauri::State;
use uuid::Uuid;
use zeroize::Zeroizing;

use super::common::{
    BrowserLoginPollRequest, BrowserLoginStartRequest, DesktopDeviceSecrets, DesktopOpaqueSuite,
    MSGPACK_CONTENT_TYPE, OpaqueSigninFinishRequest, OpaqueSigninStartRequest, SigninTotpRequest,
    clear_refresh_token_secure, clear_session_username_secure, decode_msgpack,
    decode_msgpack_bytes, encode_msgpack, endpoint, ensure_parent_dir,
    load_account_master_key_secure, load_or_create_dav_credentials, load_or_create_device_secrets,
    load_or_create_sqlite_key, load_refresh_credential_secure, load_session_username_secure,
    revoke_refresh_session, store_account_master_key_secure, store_refresh_credential_secure,
    store_refresh_token_secure, store_session_username_secure, to_ui_error,
    wrap_recovery_space_key,
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

#[derive(Debug, serde::Deserialize)]
struct RefreshResponse {
    access_token: String,
    username: String,
    refresh_token: Option<String>,
}

#[derive(serde::Deserialize, serde::Serialize)]
struct DeviceKeyPackage {
    device_id: Uuid,
    key_epoch: u32,
    #[serde(with = "serde_bytes")]
    encrypted_key_package: Vec<u8>,
}

#[derive(serde::Deserialize, serde::Serialize)]
struct SpaceSummary {
    space_id: Uuid,
    key_epoch: u32,
    history_start_seq: u64,
    current_state_start_seq: u64,
    #[serde(with = "serde_bytes")]
    encrypted_metadata: Vec<u8>,
    device_key_packages: Vec<DeviceKeyPackage>,
}

#[derive(serde::Deserialize, serde::Serialize)]
struct ListSpacesResponse {
    spaces: Vec<SpaceSummary>,
}

#[derive(serde::Deserialize, serde::Serialize)]
struct RecoverySpaceKeyPackage {
    space_id: Uuid,
    key_epoch: u32,
    #[serde(with = "serde_bytes")]
    encrypted_key_package: Vec<u8>,
}

#[derive(serde::Deserialize, serde::Serialize)]
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
        return decode_msgpack_bytes(&runner.cloud_get_msgpack(path).await.map_err(to_ui_error)?);
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

    let records = reconcile_space_records(base, access_token, master_key, &secrets, runner).await?;
    *state.collections.write().await = records;
    *state.device_identity.write().await = Some(secrets.bridge_identity());
    *state.dav_credentials.write().await = Some(load_or_create_dav_credentials(base, username)?);
    store_session_username_secure(base, username)?;
    state.set_username(Some(username.to_string())).await;
    Ok(())
}

// Platform credential access stays in provisioning; this is also used by the
// restart/reconciliation regressions with synthetic authorized identities.
async fn reconcile_space_records(
    base: &str,
    access_token: &str,
    master_key: &[u8; 32],
    secrets: &DesktopDeviceSecrets,
    runner: Option<&LocalBridgeRunner>,
) -> Result<std::collections::HashMap<String, CollectionRecord>, String> {
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
            encrypted_key_package: wrap_recovery_space_key(master_key, &key)?,
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
    Ok(records)
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

async fn refresh_startup_credential<P, F, Fut>(
    store: &crypto_core_lib::local_bridge_runner::RefreshCredentialStore,
    keychain: (String, Uuid),
    username: &str,
    persist: P,
    send: F,
) -> Result<Option<RefreshResponse>, String>
where
    P: FnOnce(&str, Uuid) -> Result<(), String>,
    F: FnOnce(RefreshRequest) -> Fut,
    Fut: std::future::Future<Output = Result<Option<RefreshResponse>, String>>,
{
    let token = store
        .recover(&keychain.0)
        .map_err(to_ui_error)?
        .map(|(_, refresh)| refresh)
        .unwrap_or_else(|| keychain.0.clone());
    let fallback = if token == keychain.0 {
        keychain.1
    } else {
        Uuid::new_v4()
    };
    let rotation_request_id = store
        .begin_with_fallback(&token, fallback)
        .map_err(to_ui_error)?;
    // Persist the selected generation before rotating it again. A crash after
    // cache commit must always leave a keychain anchor at its immediate parent.
    persist(&token, rotation_request_id)?;
    let Some(response) = send(RefreshRequest {
        refresh_token: Some(token.clone()),
        rotation_request_id,
    })
    .await?
    else {
        return Ok(None);
    };
    if response.username.trim() != username {
        return Err("refresh response account does not match stored account".into());
    }
    let refresh = response
        .refresh_token
        .as_deref()
        .filter(|token| !token.trim().is_empty())
        .ok_or_else(|| "refresh response did not contain a refresh token".to_string())?;
    store
        .commit(&token, &response.access_token, refresh)
        .map_err(to_ui_error)?;
    Ok(Some(response))
}

/// Restores a keychain-backed desktop session after process restart.
#[tauri::command]
pub async fn restore_session(state: State<'_, DesktopState>) -> Result<bool, String> {
    let base = state.cloud_base_url().await;
    let refresh_credential = load_refresh_credential_secure(&base)?;
    let stored_username = load_session_username_secure(&base)?;
    let (Some((refresh_token, rotation_request_id)), Some(stored_username)) =
        (refresh_credential, stored_username)
    else {
        // Remove an orphan half-session so later launches are deterministic.
        clear_refresh_token_secure(&base)?;
        clear_session_username_secure(&base)?;
        return Ok(false);
    };
    // Validate account-vault provenance before adopting legacy cache state or sending refresh.
    let master_key = load_account_master_key_secure(&base, &stored_username)?;
    let sqlite_path = state.sqlite_cache_path().await;
    ensure_parent_dir(&sqlite_path)?;
    let sqlite_key = load_or_create_sqlite_key(&sqlite_path)?;
    let credentials = crypto_core_lib::local_bridge_runner::RefreshCredentialStore::open(
        sqlite_path,
        sqlite_key,
        &base,
        &refresh_token,
        &stored_username,
    )
    .map_err(to_ui_error)?;
    let refresh_base = &base;
    let refreshed = refresh_startup_credential(
        &credentials,
        (refresh_token, rotation_request_id),
        &stored_username,
        |token, attempt| store_refresh_credential_secure(&base, token, attempt),
        |request| async move {
            let response = reqwest::Client::new()
                .post(endpoint(refresh_base, "/auth/refresh"))
                .header(reqwest::header::CONTENT_TYPE, MSGPACK_CONTENT_TYPE)
                .header(reqwest::header::ACCEPT, MSGPACK_CONTENT_TYPE)
                .body(encode_msgpack(&request)?)
                .send()
                .await
                .map_err(to_ui_error)?;
            if response.status() == reqwest::StatusCode::UNAUTHORIZED {
                return Ok(None);
            }
            decode_msgpack(response.error_for_status().map_err(to_ui_error)?)
                .await
                .map(Some)
        },
    )
    .await?;
    let Some(refreshed) = refreshed else {
        clear_refresh_token_secure(&base)?;
        clear_session_username_secure(&base)?;
        return Ok(false);
    };
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

async fn persist_fresh_session(
    state: &DesktopState,
    base: &str,
    username: &str,
    access: &str,
    refresh: &str,
) -> Result<(), String> {
    let path = state.sqlite_cache_path().await;
    ensure_parent_dir(&path)?;
    let key = load_or_create_sqlite_key(&path)?;
    let credentials = crypto_core_lib::local_bridge_runner::RefreshCredentialStore::start_session(
        path, key, base, username, access, refresh,
    )
    .map_err(to_ui_error)?;
    store_refresh_credential_secure(
        base,
        refresh,
        credentials.begin(refresh).map_err(to_ui_error)?,
    )
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
        if let Err(error) =
            persist_fresh_session(state.inner(), &base, &username, &token, &refresh_token).await
        {
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
    launch_browser_login(state.inner(), &response.verification_uri, |url| {
        open::that_detached(url)
    })
    .await?;
    Ok(response)
}

async fn launch_browser_login(
    state: &DesktopState,
    verification_uri: &str,
    launch: impl FnOnce(&str) -> std::io::Result<()>,
) -> Result<(), String> {
    if let Err(error) = launch(verification_uri) {
        state.pending_browser_login.lock().await.take();
        return Err(to_ui_error(error));
    }
    Ok(())
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
    if let Err(error) =
        persist_fresh_session(state.inner(), &base, username, access_token, &refresh_token).await
    {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crypto_core_lib::local_bridge_runner::LocalBridgeConfig;
    use std::io::{Read, Write};

    #[tokio::test]
    async fn failed_browser_launch_propagates_error_and_drops_pending_private_key() {
        let state = DesktopState::new("https://api.kamori.app", ":memory:");
        *state.pending_browser_login.lock().await = Some(PendingBrowserLogin {
            flow_id: Uuid::new_v4(),
            hpke_private_key: Zeroizing::new([42; 32]),
        });
        let error = launch_browser_login(
            &state,
            "https://portal.kamori.app/device?code=synthetic",
            |url| {
                assert_eq!(url, "https://portal.kamori.app/device?code=synthetic");
                Err(std::io::Error::other("synthetic launch denied"))
            },
        )
        .await
        .unwrap_err();
        assert_eq!(error, "synthetic launch denied");
        assert!(state.pending_browser_login.lock().await.is_none());
    }

    #[tokio::test]
    async fn successful_browser_launch_keeps_pending_flow_for_polling() {
        let state = DesktopState::new("https://api.kamori.app", ":memory:");
        let flow_id = Uuid::new_v4();
        *state.pending_browser_login.lock().await = Some(PendingBrowserLogin {
            flow_id,
            hpke_private_key: Zeroizing::new([42; 32]),
        });
        launch_browser_login(
            &state,
            "https://portal.kamori.app/device?code=synthetic",
            |url| {
                assert_eq!(url, "https://portal.kamori.app/device?code=synthetic");
                Ok(())
            },
        )
        .await
        .unwrap();
        let pending = state.pending_browser_login.lock().await;
        assert_eq!(pending.as_ref().unwrap().flow_id, flow_id);
        assert_eq!(
            pending.as_ref().unwrap().hpke_private_key.as_ref(),
            &[42; 32]
        );
    }

    #[tokio::test]
    async fn nonempty_cloud_spaces_decode_in_direct_and_runner_paths() {
        let space_id = Uuid::new_v4();
        let device_id = Uuid::new_v4();
        let bytes = encode_msgpack(&ListSpacesResponse {
            spaces: vec![SpaceSummary {
                space_id,
                key_epoch: 3,
                history_start_seq: 11,
                current_state_start_seq: 12,
                encrypted_metadata: vec![0, 128, 255],
                device_key_packages: vec![DeviceKeyPackage {
                    device_id,
                    key_epoch: 3,
                    encrypted_key_package: vec![255, 0, 129],
                }],
            }],
        })
        .unwrap();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let server = std::thread::spawn(move || {
            for _ in 0..2 {
                let (mut stream, _) = listener.accept().unwrap();
                let mut buffer = [0; 4096];
                let count = stream.read(&mut buffer).unwrap();
                assert!(String::from_utf8_lossy(&buffer[..count]).contains("/spaces"));
                write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: application/msgpack\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", bytes.len()).unwrap();
                stream.write_all(&bytes).unwrap();
            }
        });
        let path =
            std::env::temp_dir().join(format!("kamori-desktop-decode-{}.sqlite3", Uuid::new_v4()));
        let runner =
            LocalBridgeRunner::new(LocalBridgeConfig::new(&path, &base, "access")).unwrap();
        for transport in [None, Some(&runner)] {
            let response: ListSpacesResponse =
                authenticated_get_msgpack(transport, &base, "access", "/spaces")
                    .await
                    .expect("cloud UUID strings and MessagePack binary fields");
            assert_eq!(response.spaces[0].space_id, space_id);
            assert_eq!(
                response.spaces[0].device_key_packages[0].device_id,
                device_id
            );
            assert_eq!(
                response.spaces[0].device_key_packages[0].encrypted_key_package,
                [255, 0, 129]
            );
            assert_eq!(response.spaces[0].encrypted_metadata, [0, 128, 255]);
        }
        server.join().unwrap();
        drop(runner);
        let _ = std::fs::remove_file(path);
    }
    fn credential_store(
        path: &std::path::Path,
        token: &str,
    ) -> crypto_core_lib::local_bridge_runner::RefreshCredentialStore {
        crypto_core_lib::local_bridge_runner::RefreshCredentialStore::open(
            path,
            "synthetic-cache-key".into(),
            "http://127.0.0.1:9",
            token,
            "alice",
        )
        .unwrap()
    }

    #[tokio::test]
    async fn startup_refresh_preserves_exact_retry_and_unrelated_sessions_at_crash_boundaries() {
        for stage in ["response-lost", "cache-committed", "keychain-current"] {
            let path =
                std::env::temp_dir().join(format!("kamori-startup-{}.sqlite3", Uuid::new_v4()));
            let store = credential_store(&path, "T0");
            let runtime_attempt = store.begin("T0").unwrap();
            if stage != "response-lost" {
                store.commit("T0", "A1", "T1").unwrap();
            }
            let keychain = (
                if stage == "keychain-current" {
                    "T1"
                } else {
                    "T0"
                }
                .to_string(),
                Uuid::new_v4(),
            );
            drop(store); // process restart, OS credential can still be stale
            let store = credential_store(&path, &keychain.0);
            let mut unrelated_session_active = true;
            let mut replacement_active = true;
            let response = refresh_startup_credential(
                &store,
                keychain,
                "alice",
                |token, attempt| {
                    let expected = if stage == "response-lost" { "T0" } else { "T1" };
                    assert_eq!(token, expected);
                    assert_eq!(store.begin(token).unwrap(), attempt);
                    Ok(())
                },
                |request| {
                    let exact_retry = request.refresh_token.as_deref() == Some("T0")
                        && request.rotation_request_id == runtime_attempt;
                    let current = request.refresh_token.as_deref() == Some("T1");
                    if !exact_retry && !current {
                        unrelated_session_active = false;
                        replacement_active = false;
                    }
                    async move {
                        if !exact_retry && !current {
                            return Err("refresh_token_reuse".into());
                        }
                        Ok(Some(RefreshResponse {
                            access_token: "A2".into(),
                            username: "alice".into(),
                            refresh_token: Some(if exact_retry { "T1" } else { "T2" }.into()),
                        }))
                    }
                },
            )
            .await;
            assert!(
                response.is_ok(),
                "{stage}: startup must recover runtime credentials: {:?}",
                response.err()
            );
            assert!(
                replacement_active && unrelated_session_active,
                "{stage}: session reuse must never revoke account sessions"
            );
            drop(store);
            let _ = std::fs::remove_file(path);
        }
    }

    fn read_request(stream: &mut std::net::TcpStream) -> (String, Vec<u8>) {
        let mut bytes = Vec::new();
        let mut buffer = [0; 4096];
        loop {
            let n = stream.read(&mut buffer).unwrap();
            assert_ne!(n, 0);
            bytes.extend_from_slice(&buffer[..n]);
            if let Some(end) = bytes.windows(4).position(|b| b == b"\r\n\r\n") {
                let headers = String::from_utf8(bytes[..end].to_vec()).unwrap();
                let length = headers
                    .lines()
                    .find_map(|line| {
                        line.to_ascii_lowercase()
                            .strip_prefix("content-length:")
                            .map(|s| s.trim().parse::<usize>().unwrap())
                    })
                    .unwrap_or(0);
                if bytes.len() >= end + 4 + length {
                    return (headers, bytes[end + 4..end + 4 + length].to_vec());
                }
            }
        }
    }

    fn respond(stream: &mut std::net::TcpStream, status: &str, body: &[u8]) {
        write!(stream, "HTTP/1.1 {status}\r\nContent-Type: application/msgpack\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len()).unwrap();
        stream.write_all(body).unwrap();
    }

    #[tokio::test]
    async fn runner_reconciliation_repairs_legacy_recovery_only_with_current_device_key() {
        for authorized_current_epoch in [true, false] {
            let master = [21; 32];
            let key = [33; 32];
            let identity = CryptoEngine::generate_x25519_keypair();
            let secrets = DesktopDeviceSecrets {
                device_id: Uuid::new_v4(),
                hpke_private_key: identity.private_key,
                hpke_public_key: identity.public_key,
                signing_private_key: [42; 32],
            };
            let space_id = Uuid::new_v4();
            let spaces = encode_msgpack(&ListSpacesResponse {
                spaces: vec![SpaceSummary {
                    space_id,
                    key_epoch: 2,
                    history_start_seq: 17,
                    current_state_start_seq: 23,
                    encrypted_metadata: secret_vault::encrypt(
                        &key,
                        &rmp_serde::to_vec_named(&serde_json::json!({"name":"Repaired"})).unwrap(),
                    )
                    .unwrap(),
                    device_key_packages: vec![DeviceKeyPackage {
                        device_id: secrets.device_id,
                        key_epoch: if authorized_current_epoch { 2 } else { 1 },
                        encrypted_key_package: rmp_serde::to_vec_named(
                            &CryptoEngine::encrypt_group_key_for_peer(&key, &identity.public_key)
                                .unwrap(),
                        )
                        .unwrap(),
                    }],
                }],
            })
            .unwrap();
            let legacy = secret_vault::encrypt(&master, &key).unwrap();
            let recovery = encode_msgpack(&ListRecoveryKeyPackagesResponse {
                packages: vec![RecoverySpaceKeyPackage {
                    space_id,
                    key_epoch: 2,
                    encrypted_key_package: legacy,
                }],
            })
            .unwrap();
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let base = format!("http://{}", listener.local_addr().unwrap());
            let server = std::thread::spawn(move || {
                let mut requests = vec![
                    ("/spaces".to_string(), spaces),
                    ("/spaces/recovery-key-packages".to_string(), recovery),
                ];
                if authorized_current_epoch {
                    requests.push((
                        format!("/spaces/{space_id}/recovery-key-package"),
                        Vec::new(),
                    ));
                }
                for (path, response) in requests {
                    let (mut stream, _) = listener.accept().unwrap();
                    let (headers, body) = read_request(&mut stream);
                    assert!(headers.lines().next().unwrap().contains(&path));
                    if !body.is_empty() {
                        #[derive(serde::Deserialize)]
                        struct Package {
                            key_epoch: u32,
                            #[serde(with = "serde_bytes")]
                            encrypted_key_package: Vec<u8>,
                        }
                        let package: Package = decode_msgpack_bytes(&body).unwrap();
                        assert_eq!(package.key_epoch, 2);
                        let encrypted: EncryptedGroupKey =
                            rmp_serde::from_slice(&package.encrypted_key_package).unwrap();
                        let recovery = CryptoEngine::derive_account_recovery_keypair(&master);
                        assert_eq!(
                            CryptoEngine::decrypt_group_key_from_peer(
                                &encrypted,
                                &recovery.private_key
                            )
                            .unwrap(),
                            key
                        );
                    }
                    respond(&mut stream, "200 OK", &response);
                }
            });
            let path =
                std::env::temp_dir().join(format!("kamori-reconcile-{}.sqlite3", Uuid::new_v4()));
            let runner =
                LocalBridgeRunner::new(LocalBridgeConfig::new(&path, &base, "access")).unwrap();
            let records =
                reconcile_space_records(&base, "access", &master, &secrets, Some(&runner)).await;
            if authorized_current_epoch {
                let records = records.unwrap();
                let record = &records[&space_id.to_string()];
                assert_eq!(record.name, "Repaired");
                assert_eq!(record.cmk, key);
                assert_eq!(record.key_epoch, 2);
                assert_eq!(record.sync_start_seq, 23);
            } else {
                assert!(
                    records.is_err(),
                    "an old epoch device key must not repair unavailable current recovery material"
                );
            }
            server.join().unwrap();
            drop(runner);
            let _ = std::fs::remove_file(path);
        }
    }

    #[tokio::test]
    async fn runtime_dropped_refresh_response_is_exactly_retried_by_startup() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            read_request(&mut stream);
            respond(&mut stream, "401 Unauthorized", &[]);
            let (mut stream, _) = listener.accept().unwrap();
            let (headers, body) = read_request(&mut stream);
            assert!(headers.starts_with("POST /auth/refresh "));
            #[derive(serde::Deserialize)]
            struct Attempt {
                refresh_token: String,
                rotation_request_id: Uuid,
            }
            let request: Attempt = decode_msgpack_bytes(&body).unwrap();
            assert_eq!(request.refresh_token, "T0");
            request.rotation_request_id // server consumes T0, then drops response
        });
        let path =
            std::env::temp_dir().join(format!("kamori-runtime-crash-{}.sqlite3", Uuid::new_v4()));
        let runner = LocalBridgeRunner::new(
            LocalBridgeConfig::new(&path, &base, "expired")
                .with_sqlite_key("synthetic-key")
                .with_refresh_token("T0"),
        )
        .unwrap();
        assert!(runner.cloud_get_msgpack("/spaces").await.is_err());
        let runtime_attempt = server.join().unwrap();
        drop(runner);
        let store = crypto_core_lib::local_bridge_runner::RefreshCredentialStore::open(
            &path,
            "synthetic-key".into(),
            &base,
            "T0",
            "alice",
        )
        .unwrap();
        let stale_keychain = ("T0".into(), Uuid::new_v4());
        refresh_startup_credential(
            &store,
            stale_keychain,
            "alice",
            |_, _| Ok(()),
            |request| async move {
                assert_eq!(request.refresh_token.as_deref(), Some("T0"));
                assert_eq!(
                    request.rotation_request_id, runtime_attempt,
                    "different id would revoke all account sessions"
                );
                Ok(Some(RefreshResponse {
                    access_token: "A1".into(),
                    refresh_token: Some("T1".into()),
                    username: "alice".into(),
                }))
            },
        )
        .await
        .unwrap();
        // Crash after T1 cache commit/keychain T0: the next attempt first
        // advances the OS anchor to T1, then loses its refresh response again.
        let persisted = std::cell::RefCell::new(String::from("T0"));
        let next_attempt = std::cell::Cell::new(Uuid::nil());
        let lost = refresh_startup_credential(
            &store,
            ("T0".into(), Uuid::new_v4()),
            "alice",
            |token, _| {
                *persisted.borrow_mut() = token.to_string();
                Ok(())
            },
            |request| {
                assert_eq!(persisted.borrow().as_str(), "T1");
                assert_eq!(request.refresh_token.as_deref(), Some("T1"));
                next_attempt.set(request.rotation_request_id);
                async { Err("response lost after second rotation".into()) }
            },
        )
        .await;
        assert!(lost.is_err());
        refresh_startup_credential(
            &store,
            (persisted.into_inner(), Uuid::new_v4()),
            "alice",
            |_, _| Ok(()),
            |request| async move {
                assert_eq!(request.refresh_token.as_deref(), Some("T1"));
                assert_eq!(request.rotation_request_id, next_attempt.get());
                Ok(Some(RefreshResponse {
                    access_token: "A2".into(),
                    refresh_token: Some("T2".into()),
                    username: "alice".into(),
                }))
            },
        )
        .await
        .unwrap();
        drop(store);
        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn startup_rejects_response_for_another_account_without_committing_it() {
        let path = std::env::temp_dir().join(format!("kamori-account-{}.sqlite3", Uuid::new_v4()));
        let store = credential_store(&path, "T0");
        let result = refresh_startup_credential(
            &store,
            ("T0".into(), Uuid::new_v4()),
            "alice",
            |_, _| Ok(()),
            |_| async {
                Ok(Some(RefreshResponse {
                    access_token: "bob-access".into(),
                    refresh_token: Some("bob-refresh".into()),
                    username: "bob".into(),
                }))
            },
        )
        .await;
        assert!(result.unwrap_err().contains("account does not match"));
        assert!(store.recover("T0").unwrap().is_none());
        drop(store);
        let _ = std::fs::remove_file(path);
    }
    #[tokio::test]
    async fn startup_keychain_failure_stops_network_and_preserves_retry() {
        let path =
            std::env::temp_dir().join(format!("kamori-anchor-failure-{}.sqlite3", Uuid::new_v4()));
        let store = credential_store(&path, "T0");
        store.commit("T0", "expired-A1", "T1").unwrap();
        let result = refresh_startup_credential(
            &store,
            ("T0".into(), Uuid::new_v4()),
            "alice",
            |token, _| {
                assert_eq!(token, "T1");
                Err("fake OS storage failure".into())
            },
            |_| async { panic!("no refresh before durable keychain anchor") },
        )
        .await;
        assert_eq!(result.unwrap_err(), "fake OS storage failure");
        assert_eq!(store.recover("T0").unwrap().unwrap().1, "T1");
        let request_id = store.begin("T1").unwrap();
        refresh_startup_credential(
            &store,
            ("T0".into(), Uuid::new_v4()),
            "alice",
            |_, _| Ok(()),
            |request| async move {
                assert_eq!(request.rotation_request_id, request_id);
                Ok(Some(RefreshResponse {
                    access_token: "A2".into(),
                    refresh_token: Some("T2".into()),
                    username: "alice".into(),
                }))
            },
        )
        .await
        .unwrap();
        drop(store);
        let _ = std::fs::remove_file(path);
    }
    #[tokio::test]
    async fn startup_keychain_only_attempt_is_durably_adopted_before_network() {
        let path =
            std::env::temp_dir().join(format!("kamori-keychain-only-{}.sqlite3", Uuid::new_v4()));
        let keychain_attempt = Uuid::new_v4();
        let store = credential_store(&path, "T0");
        let persisted = std::cell::Cell::new(false);
        let result = refresh_startup_credential(
            &store,
            ("T0".into(), keychain_attempt),
            "alice",
            |token, attempt| {
                assert_eq!(token, "T0");
                assert_eq!(attempt, keychain_attempt);
                assert_eq!(store.begin(token).unwrap(), attempt);
                persisted.set(true);
                Ok(())
            },
            |request| {
                assert!(persisted.get());
                assert_eq!(request.rotation_request_id, keychain_attempt);
                async { Err("response lost".into()) }
            },
        )
        .await;
        assert!(result.is_err());
        drop(store);
        let store = credential_store(&path, "T0");
        assert_eq!(store.begin("T0").unwrap(), keychain_attempt);
        drop(store);
        let _ = std::fs::remove_file(path);
    }
    #[tokio::test]
    async fn startup_refuses_older_than_predecessor_until_explicit_fresh_signin() {
        use crypto_core_lib::local_bridge_runner::RefreshCredentialStore;
        let path =
            std::env::temp_dir().join(format!("kamori-old-anchor-{}.sqlite3", Uuid::new_v4()));
        let base = "http://127.0.0.1:9";
        let store = credential_store(&path, "T0");
        let runner = LocalBridgeRunner::new(
            LocalBridgeConfig::new(&path, base, "A0")
                .with_sqlite_key("synthetic-cache-key")
                .with_refresh_token("T0"),
        )
        .unwrap();
        runner
            .persist_rotated_credentials("T0".into(), "A1".into(), "T1".into())
            .await
            .unwrap();
        runner
            .persist_rotated_credentials("T1".into(), "A2".into(), "T2".into())
            .await
            .unwrap();
        drop(runner);
        drop(store);
        let stale =
            RefreshCredentialStore::open(&path, "synthetic-cache-key".into(), base, "T0", "alice");
        assert!(
            stale.is_err(),
            "production startup must stop before sending unknown old T0"
        );
        // A fresh authenticated signin explicitly establishes a new lineage,
        // even for a different account/backend using the same fixed cache path.
        let store = RefreshCredentialStore::start_session(
            &path,
            "synthetic-cache-key".into(),
            "https://new.example",
            "bob",
            "new-access",
            "new-refresh",
        )
        .unwrap();
        assert!(store.recover("T0").unwrap().is_none());
        drop(store);
        let store = RefreshCredentialStore::open(
            &path,
            "synthetic-cache-key".into(),
            "https://new.example",
            "new-refresh",
            "bob",
        )
        .unwrap();
        refresh_startup_credential(
            &store,
            ("new-refresh".into(), Uuid::new_v4()),
            "bob",
            |_, _| Ok(()),
            |request| async move {
                assert_eq!(request.refresh_token.as_deref(), Some("new-refresh"));
                Ok(Some(RefreshResponse {
                    access_token: "new-access2".into(),
                    refresh_token: Some("new-refresh2".into()),
                    username: "bob".into(),
                }))
            },
        )
        .await
        .unwrap();
        drop(store);
        let _ = std::fs::remove_file(path);
    }
}
