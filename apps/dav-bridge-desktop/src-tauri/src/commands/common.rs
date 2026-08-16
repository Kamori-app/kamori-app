//! Shared command helpers and request payload models.
use crypto_core_lib::local_bridge_runner::LocalBridgeConfig;
use keyring::{Entry, Error as KeyringError};
use opaque_ke::{
    Ristretto255, argon2::Argon2, ciphersuite::CipherSuite, key_exchange::tripledh::TripleDh,
};
use rand_core::{OsRng, RngCore};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use sha2::{Digest, Sha256};
use sha2_opaque::Sha512;
use std::path::Path;
use tauri::{AppHandle, Manager};
use uuid::Uuid;

/// Main window label used by runtime and tray interactions.
pub(super) const MAIN_WINDOW_LABEL: &str = "main";
/// Stable tray icon id for create/remove operations.
pub(super) const MAIN_TRAY_ID: &str = "main-tray";
/// MessagePack media type used by cloud API calls.
pub(super) const MSGPACK_CONTENT_TYPE: &str = "application/msgpack";

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) struct DesktopDeviceSecrets {
    pub(super) device_id: Uuid,
    pub(super) signing_private_key: [u8; 32],
    pub(super) hpke_private_key: [u8; 32],
    pub(super) hpke_public_key: [u8; 32],
}

impl DesktopDeviceSecrets {
    pub(super) fn signing_public_key(&self) -> [u8; 32] {
        *ed25519_dalek::SigningKey::from_bytes(&self.signing_private_key)
            .verifying_key()
            .as_bytes()
    }

    pub(super) fn bridge_identity(
        &self,
    ) -> crypto_core_lib::local_bridge_runner::LocalDeviceIdentity {
        crypto_core_lib::local_bridge_runner::LocalDeviceIdentity {
            device_id: self.device_id,
            signing_private_key: self.signing_private_key,
        }
    }
}

#[derive(Debug, Serialize)]
pub(super) struct OpaqueSigninStartRequest {
    pub(super) username: String,
    #[serde(with = "serde_bytes")]
    pub(super) opaque_start_request: Vec<u8>,
}

#[derive(Debug, Serialize)]
pub(super) struct OpaqueSigninFinishRequest {
    pub(super) username: String,
    pub(super) opaque_flow_id: Uuid,
    #[serde(with = "serde_bytes")]
    pub(super) opaque_finish_request: Vec<u8>,
    pub(super) totp_code: Option<String>,
    pub(super) preauth_token: Option<String>,
}

#[derive(Debug, Serialize)]
pub(super) struct PasskeyLoginStartRequest {}

#[derive(Debug, Serialize)]
pub(super) struct PasskeyLoginFinishRequest {
    pub(super) flow_id: Uuid,
    #[serde(with = "serde_bytes")]
    pub(super) credential: Vec<u8>,
}

/// OPAQUE ciphersuite used by desktop client to match cloud-server.
pub(super) struct DesktopOpaqueSuite;

impl CipherSuite for DesktopOpaqueSuite {
    type OprfCs = Ristretto255;
    type KeyExchange = TripleDh<Ristretto255, Sha512>;
    type Ksf = Argon2<'static>;
}

/// Converts any displayable error into a user-facing string.
pub(super) fn to_ui_error(err: impl std::fmt::Display) -> String {
    err.to_string()
}

/// Encodes a payload into MessagePack bytes.
pub(super) fn encode_msgpack<T: Serialize>(value: &T) -> Result<Vec<u8>, String> {
    rmp_serde::to_vec_named(value).map_err(to_ui_error)
}

/// Decodes a MessagePack response into the expected model.
pub(super) async fn decode_msgpack<T: DeserializeOwned>(
    response: reqwest::Response,
) -> Result<T, String> {
    let bytes = response.bytes().await.map_err(to_ui_error)?;
    rmp_serde::from_slice(&bytes).map_err(to_ui_error)
}

/// Joins base url and endpoint path into a stable absolute URL.
pub(super) fn endpoint(base: &str, path: &str) -> String {
    format!(
        "{}/{}",
        base.trim_end_matches('/'),
        path.trim_start_matches('/')
    )
}

/// Ensures that parent directory for a target file path exists.
pub(super) fn ensure_parent_dir(path: &str) -> Result<(), String> {
    let parent = Path::new(path).parent();
    if let Some(dir) = parent
        && !dir.as_os_str().is_empty()
    {
        std::fs::create_dir_all(dir).map_err(to_ui_error)?;
    }
    Ok(())
}

fn hex_encode(input: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(input.len() * 2);
    for byte in input {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}

fn hex_decode(input: &str) -> Result<Vec<u8>, String> {
    if !input.len().is_multiple_of(2) {
        return Err("secure value has invalid hex length".to_string());
    }
    input
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let text = std::str::from_utf8(pair).map_err(to_ui_error)?;
            u8::from_str_radix(text, 16).map_err(to_ui_error)
        })
        .collect()
}

fn scoped_keychain_account(prefix: &str, cloud_base_url: &str, username: &str) -> String {
    let scope = format!(
        "{}:{}",
        cloud_base_url.trim().to_ascii_lowercase(),
        username.trim().to_ascii_lowercase()
    );
    format!("{prefix}:{}", hex_encode(&Sha256::digest(scope.as_bytes())))
}

pub(super) fn load_or_create_device_secrets(
    cloud_base_url: &str,
    username: &str,
) -> Result<DesktopDeviceSecrets, String> {
    let entry = Entry::new(
        "app.kamori.desktop.device",
        &scoped_keychain_account("device", cloud_base_url, username),
    )
    .map_err(to_ui_error)?;
    match entry.get_password() {
        Ok(encoded) => rmp_serde::from_slice(&hex_decode(encoded.trim())?).map_err(to_ui_error),
        Err(KeyringError::NoEntry) => {
            let hpke = crypto_core_lib::CryptoEngine::generate_x25519_keypair();
            let mut signing_private_key = [0_u8; 32];
            OsRng.fill_bytes(&mut signing_private_key);
            let secrets = DesktopDeviceSecrets {
                device_id: Uuid::new_v4(),
                signing_private_key,
                hpke_private_key: hpke.private_key,
                hpke_public_key: hpke.public_key,
            };
            let encoded = rmp_serde::to_vec_named(&secrets).map_err(to_ui_error)?;
            entry
                .set_password(&hex_encode(&encoded))
                .map_err(to_ui_error)?;
            Ok(secrets)
        }
        Err(error) => Err(format!("failed to load device identity: {error}")),
    }
}

pub(super) fn store_account_master_key_secure(
    cloud_base_url: &str,
    username: &str,
    master_key: &[u8; 32],
) -> Result<(), String> {
    Entry::new(
        "app.kamori.desktop.account-master",
        &scoped_keychain_account("account-master", cloud_base_url, username),
    )
    .map_err(to_ui_error)?
    .set_password(&hex_encode(master_key))
    .map_err(|error| format!("failed to store account master key: {error}"))
}

pub(super) fn load_account_master_key_secure(
    cloud_base_url: &str,
    username: &str,
) -> Result<[u8; 32], String> {
    let value = Entry::new(
        "app.kamori.desktop.account-master",
        &scoped_keychain_account("account-master", cloud_base_url, username),
    )
    .map_err(to_ui_error)?
    .get_password()
    .map_err(|error| format!("unlock this device once with your password: {error}"))?;
    hex_decode(value.trim())?
        .try_into()
        .map_err(|_| "stored account master key has invalid length".to_string())
}

pub(super) fn load_or_create_dav_credentials(
    cloud_base_url: &str,
    username: &str,
) -> Result<(String, String), String> {
    let entry = Entry::new(
        "app.kamori.desktop.dav",
        &scoped_keychain_account("dav", cloud_base_url, username),
    )
    .map_err(to_ui_error)?;
    match entry.get_password() {
        Ok(value) => value
            .split_once(':')
            .map(|(username, password)| (username.to_string(), password.to_string()))
            .filter(|(_, password)| password.len() >= 32)
            .ok_or_else(|| "stored DAV credential is invalid".to_string()),
        Err(KeyringError::NoEntry) => {
            let credentials = new_dav_credentials();
            entry
                .set_password(&format!("{}:{}", credentials.0, credentials.1))
                .map_err(to_ui_error)?;
            Ok(credentials)
        }
        Err(error) => Err(format!("failed to load DAV credential: {error}")),
    }
}

pub(super) fn rotate_dav_credentials(
    cloud_base_url: &str,
    username: &str,
) -> Result<(String, String), String> {
    let entry = Entry::new(
        "app.kamori.desktop.dav",
        &scoped_keychain_account("dav", cloud_base_url, username),
    )
    .map_err(to_ui_error)?;
    let credentials = new_dav_credentials();
    entry
        .set_password(&format!("{}:{}", credentials.0, credentials.1))
        .map_err(to_ui_error)?;
    Ok(credentials)
}

fn new_dav_credentials() -> (String, String) {
    let mut secret = [0_u8; 24];
    OsRng.fill_bytes(&mut secret);
    ("kamori".to_string(), hex_encode(&secret))
}

/// Loads or creates a persistent SQLCipher key in the system keychain.
pub(super) fn load_or_create_sqlite_key(sqlite_path: &str) -> Result<String, String> {
    const KEYCHAIN_SERVICE: &str = "app.kamori.davbridge.local-sqlite";

    let account_digest = Sha256::digest(sqlite_path.as_bytes());
    let account = format!("sqlite-cache-key:{}", hex_encode(&account_digest));

    let entry = Entry::new(KEYCHAIN_SERVICE, &account)
        .map_err(|error| format!("failed to initialize system keychain entry: {error}"))?;

    match entry.get_password() {
        Ok(value) => {
            let key = value.trim().to_string();
            if key.is_empty() {
                return Err("system keychain returned empty sqlite key".to_string());
            }
            Ok(key)
        }
        Err(KeyringError::NoEntry) => {
            let mut random = [0_u8; 32];
            OsRng.fill_bytes(&mut random);
            let key = hex_encode(&random);
            entry.set_password(&key).map_err(|error| {
                format!("failed to store sqlite key in system keychain: {error}")
            })?;
            Ok(key)
        }
        Err(error) => Err(format!(
            "failed to read sqlite key from system keychain: {error}"
        )),
    }
}

fn refresh_token_account(cloud_base_url: &str) -> String {
    let normalized = cloud_base_url.trim().to_ascii_lowercase();
    let digest = Sha256::digest(normalized.as_bytes());
    format!("refresh-token:{}", hex_encode(&digest))
}

fn refresh_token_entry(cloud_base_url: &str) -> Result<Entry, String> {
    const KEYCHAIN_SERVICE: &str = "app.kamori.davbridge.refresh-token";
    let account = refresh_token_account(cloud_base_url);
    Entry::new(KEYCHAIN_SERVICE, &account)
        .map_err(|error| format!("failed to initialize refresh token keychain entry: {error}"))
}

/// Stores refresh token in OS secure keychain storage.
pub(super) fn store_refresh_token_secure(cloud_base_url: &str, token: &str) -> Result<(), String> {
    let token = token.trim();
    if token.is_empty() {
        return Err("refresh token must not be empty".to_string());
    }
    let entry = refresh_token_entry(cloud_base_url)?;
    entry
        .set_password(token)
        .map_err(|error| format!("failed to store refresh token in keychain: {error}"))
}

/// Loads refresh token from OS secure keychain storage.
pub(super) fn load_refresh_token_secure(cloud_base_url: &str) -> Result<Option<String>, String> {
    let entry = refresh_token_entry(cloud_base_url)?;
    match entry.get_password() {
        Ok(value) => {
            let token = value.trim().to_string();
            if token.is_empty() {
                return Ok(None);
            }
            Ok(Some(token))
        }
        Err(KeyringError::NoEntry) => Ok(None),
        Err(error) => Err(format!(
            "failed to read refresh token from keychain: {error}"
        )),
    }
}

/// Removes refresh token from OS secure keychain storage. Idempotent.
pub(super) fn clear_refresh_token_secure(cloud_base_url: &str) -> Result<(), String> {
    let entry = refresh_token_entry(cloud_base_url)?;
    match entry.delete_credential() {
        Ok(()) | Err(KeyringError::NoEntry) => Ok(()),
        Err(error) => Err(format!(
            "failed to remove refresh token from keychain: {error}"
        )),
    }
}

/// Applies local bridge sqlite key to config in a single place.
pub(super) fn with_sqlite_key(config: LocalBridgeConfig, sqlite_key: String) -> LocalBridgeConfig {
    config.with_sqlite_key(sqlite_key)
}

/// Restores and focuses main window from tray context.
pub(super) fn reveal_main_window(app: &AppHandle) -> Result<(), String> {
    let window = app
        .get_webview_window(MAIN_WINDOW_LABEL)
        .ok_or_else(|| "main window is not available".to_string())?;
    window.show().map_err(to_ui_error)?;
    window.unminimize().map_err(to_ui_error)?;
    window.set_focus().map_err(to_ui_error)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn endpoint_join_is_stable() {
        assert_eq!(
            endpoint("http://localhost:3000/", "/operations"),
            "http://localhost:3000/operations"
        );
    }

    #[test]
    fn ui_error_conversion_is_non_empty() {
        let msg = to_ui_error(anyhow::anyhow!("oops"));
        assert!(msg.contains("oops"));
    }

    #[test]
    fn refresh_token_account_is_stable_for_base_url() {
        let one = refresh_token_account("HTTP://LOCALHOST:3000/");
        let two = refresh_token_account("http://localhost:3000/");
        assert_eq!(one, two);
        assert!(one.starts_with("refresh-token:"));
    }
}
