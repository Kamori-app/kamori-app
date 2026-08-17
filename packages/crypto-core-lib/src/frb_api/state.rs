use std::collections::HashMap;

use once_cell::sync::Lazy;
use tokio::sync::Mutex;

use super::types::{MobileDeviceSecrets, MobileSyncRuntime};

pub(super) type MobileCollectionKeys = HashMap<String, (u32, [u8; 32])>;

pub(super) static MOBILE_BRIDGE_RUNTIME: Lazy<Mutex<MobileSyncRuntime>> =
    Lazy::new(|| Mutex::new(MobileSyncRuntime::default()));
pub(super) static MOBILE_COLLECTION_KEYS: Lazy<Mutex<MobileCollectionKeys>> =
    Lazy::new(|| Mutex::new(HashMap::new()));
pub(super) static MOBILE_REFRESH_TOKEN: Lazy<Mutex<Option<String>>> =
    Lazy::new(|| Mutex::new(None));
pub(super) static MOBILE_DEVICE_SECRETS: Lazy<Mutex<Option<MobileDeviceSecrets>>> =
    Lazy::new(|| Mutex::new(None));
pub(super) static MOBILE_ACCOUNT_MASTER_KEY: Lazy<Mutex<Option<[u8; 32]>>> =
    Lazy::new(|| Mutex::new(None));

pub(super) async fn set_mobile_refresh_token(refresh_token: Option<String>) {
    let mut stored_refresh = MOBILE_REFRESH_TOKEN.lock().await;
    *stored_refresh = refresh_token;
}
