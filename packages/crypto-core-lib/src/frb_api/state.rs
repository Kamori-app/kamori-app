use std::collections::HashMap;

use once_cell::sync::Lazy;
use tokio::sync::Mutex;
use uuid::Uuid;
use zeroize::Zeroize;

use super::types::{MobileDeviceSecrets, MobileSyncRuntime};
use crate::local_bridge_runner::LocalBridgeRunner;

pub(super) type MobileCollectionKeys = HashMap<String, (u32, [u8; 32])>;

pub(super) static MOBILE_BRIDGE_RUNTIME: Lazy<Mutex<MobileSyncRuntime>> =
    Lazy::new(|| Mutex::new(MobileSyncRuntime::default()));
pub(super) static MOBILE_RUNNER: Lazy<Mutex<Option<LocalBridgeRunner>>> =
    Lazy::new(|| Mutex::new(None));
pub(super) static MOBILE_RUNTIME_LEASE: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));
pub(super) static MOBILE_COLLECTION_KEYS: Lazy<Mutex<MobileCollectionKeys>> =
    Lazy::new(|| Mutex::new(HashMap::new()));
pub(super) static MOBILE_SYNC_STARTS: Lazy<Mutex<HashMap<String, u64>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));
pub(super) static MOBILE_REFRESH_TOKEN: Lazy<Mutex<Option<String>>> =
    Lazy::new(|| Mutex::new(None));
pub(super) static MOBILE_REFRESH_ROTATION_REQUEST_ID: Lazy<Mutex<Option<Uuid>>> =
    Lazy::new(|| Mutex::new(None));
pub(super) static MOBILE_DEVICE_SECRETS: Lazy<Mutex<Option<MobileDeviceSecrets>>> =
    Lazy::new(|| Mutex::new(None));
pub(super) static MOBILE_ACCOUNT_MASTER_KEY: Lazy<Mutex<Option<[u8; 32]>>> =
    Lazy::new(|| Mutex::new(None));
pub(super) static MOBILE_PENDING_TOTP_LOGIN: Lazy<Mutex<Option<MobilePendingTotpLogin>>> =
    Lazy::new(|| Mutex::new(None));

#[derive(Clone)]
pub(super) struct MobilePendingTotpLogin {
    pub(super) username: String,
    pub(super) continuation_token: String,
    pub(super) export_key: Vec<u8>,
}

pub(super) async fn set_mobile_refresh_token(refresh_token: Option<String>) {
    let mut stored_refresh = MOBILE_REFRESH_TOKEN.lock().await;
    let changed = *stored_refresh != refresh_token;
    if let Some(current) = stored_refresh.as_mut() {
        current.zeroize();
    }
    *stored_refresh = refresh_token;
    if changed {
        let mut request_id = MOBILE_REFRESH_ROTATION_REQUEST_ID.lock().await;
        *request_id = stored_refresh.as_ref().map(|_| Uuid::new_v4());
    }
}

pub(super) async fn import_mobile_refresh_credential(
    refresh_token: String,
    rotation_request_id: Uuid,
) {
    let mut stored_refresh = MOBILE_REFRESH_TOKEN.lock().await;
    if let Some(current) = stored_refresh.as_mut() {
        current.zeroize();
    }
    *stored_refresh = Some(refresh_token);
    *MOBILE_REFRESH_ROTATION_REQUEST_ID.lock().await = Some(rotation_request_id);
}

pub(super) async fn mobile_refresh_rotation_request_id(refresh_token: &str) -> Result<Uuid, String> {
    let stored_refresh = MOBILE_REFRESH_TOKEN.lock().await;
    if stored_refresh.as_deref() != Some(refresh_token) {
        return Err("refresh credential generation does not match the active token".to_string());
    }
    let mut request_id = MOBILE_REFRESH_ROTATION_REQUEST_ID.lock().await;
    Ok(*request_id.get_or_insert_with(Uuid::new_v4))
}

pub(super) async fn clear_mobile_runtime() -> Result<(), String> {
    let runner = { MOBILE_RUNNER.lock().await.take() };
    let clear_result = if let Some(runner) = runner {
        runner
            .clear_persisted_credentials()
            .await
            .map_err(|error| error.to_string())
    } else {
        Ok(())
    };
    let mut runtime = MOBILE_BRIDGE_RUNTIME.lock().await;
    if let Some(config) = runtime.last_config.as_mut() {
        config.access_token.zeroize();
        config.sqlite_key.fill(0);
    }
    runtime.last_config = None;
    clear_result
}

pub(super) async fn persist_mobile_rotated_credentials(
    previous_refresh_token: &str,
    access_token: &str,
    refresh_token: &str,
) -> Result<(), String> {
    let runner = { MOBILE_RUNNER.lock().await.clone() };
    if let Some(runner) = runner {
        runner
            .persist_rotated_credentials(
                previous_refresh_token.to_string(),
                access_token.to_string(),
                refresh_token.to_string(),
            )
            .await
            .map_err(|error| error.to_string())?;
    }
    if let Some(config) = MOBILE_BRIDGE_RUNTIME.lock().await.last_config.as_mut() {
        config.access_token = access_token.to_string();
    }
    set_mobile_refresh_token(Some(refresh_token.to_string())).await;
    // A direct FRB transport and the runner must never continue with different
    // credential generations. The next call rebuilds one shared runner from
    // the durably committed pair.
    MOBILE_RUNNER.lock().await.take();
    Ok(())
}
