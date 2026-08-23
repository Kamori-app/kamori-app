use crate::models::{CollectionSummary, DashboardSnapshot, LocalServerStatus};
use anyhow::{Result, anyhow};
use crypto_core_lib::local_bridge_runner::{LocalBridgeRunner, LocalDeviceIdentity};
use std::{
    collections::{BTreeMap, HashMap},
    sync::{Arc, RwLock as StdRwLock},
};
use tokio::sync::{Mutex, RwLock};
use tokio::task::JoinHandle;
use zeroize::{Zeroize, Zeroizing};

pub const FIXED_SQLITE_CACHE_PATH: &str = ".kamori/local-cache.sqlite3";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CloseBehavior {
    Quit,
    Hide,
    Minimize,
}

impl CloseBehavior {
    pub fn from_value(value: &str) -> Option<Self> {
        match value {
            "quit" => Some(Self::Quit),
            "hide" => Some(Self::Hide),
            "minimize" => Some(Self::Minimize),
            _ => None,
        }
    }
}

#[derive(Clone)]
pub struct DesktopState {
    pub cloud_base_url: Arc<RwLock<String>>,
    pub sqlite_cache_path: Arc<RwLock<String>>,
    pub access_token: Arc<RwLock<Option<String>>>,
    pub refresh_token: Arc<RwLock<Option<String>>>,
    pub username: Arc<RwLock<Option<String>>>,
    pub bridge: Arc<Mutex<Option<LocalBridgeRunner>>>,
    pub sync_task: Arc<Mutex<Option<JoinHandle<()>>>>,
    pub collections: Arc<RwLock<HashMap<String, CollectionRecord>>>,
    pub device_identity: Arc<RwLock<Option<LocalDeviceIdentity>>>,
    pub dav_credentials: Arc<RwLock<Option<(String, String)>>>,
    pub pending_totp_login: Arc<Mutex<Option<PendingTotpLogin>>>,
    pub pending_browser_login: Arc<Mutex<Option<PendingBrowserLogin>>>,
    close_behavior: Arc<StdRwLock<CloseBehavior>>,
}

#[derive(Clone)]
pub struct PendingTotpLogin {
    pub username: String,
    pub continuation_token: String,
    pub export_key: Vec<u8>,
}

pub struct PendingBrowserLogin {
    pub flow_id: uuid::Uuid,
    pub hpke_private_key: Zeroizing<[u8; 32]>,
}

#[derive(Clone)]
pub struct CollectionRecord {
    pub id: String,
    pub name: String,
    pub cmk: [u8; 32],
    pub key_epoch: u32,
    pub sync_start_seq: u64,
    pub synced_items: u64,
}

impl DesktopState {
    pub fn new(
        default_cloud_base_url: impl Into<String>,
        default_sqlite_path: impl Into<String>,
    ) -> Self {
        Self {
            cloud_base_url: Arc::new(RwLock::new(default_cloud_base_url.into())),
            sqlite_cache_path: Arc::new(RwLock::new(default_sqlite_path.into())),
            access_token: Arc::new(RwLock::new(None)),
            refresh_token: Arc::new(RwLock::new(None)),
            username: Arc::new(RwLock::new(None)),
            bridge: Arc::new(Mutex::new(None)),
            sync_task: Arc::new(Mutex::new(None)),
            collections: Arc::new(RwLock::new(HashMap::new())),
            device_identity: Arc::new(RwLock::new(None)),
            dav_credentials: Arc::new(RwLock::new(None)),
            pending_totp_login: Arc::new(Mutex::new(None)),
            pending_browser_login: Arc::new(Mutex::new(None)),
            close_behavior: Arc::new(StdRwLock::new(CloseBehavior::Quit)),
        }
    }

    pub async fn cloud_base_url(&self) -> String {
        self.cloud_base_url.read().await.clone()
    }

    pub async fn sqlite_cache_path(&self) -> String {
        self.sqlite_cache_path.read().await.clone()
    }

    pub async fn require_access_token(&self) -> Result<String> {
        self.access_token
            .read()
            .await
            .clone()
            .ok_or_else(|| anyhow!("not authenticated"))
    }

    pub async fn set_access_token(&self, token: Option<String>) {
        let mut stored = self.access_token.write().await;
        if let Some(current) = stored.as_mut() {
            current.zeroize();
        }
        *stored = token;
    }

    pub async fn refresh_token(&self) -> Option<String> {
        self.refresh_token.read().await.clone()
    }

    pub async fn set_refresh_token(&self, token: Option<String>) {
        let mut stored = self.refresh_token.write().await;
        if let Some(current) = stored.as_mut() {
            current.zeroize();
        }
        *stored = token;
    }

    pub async fn username(&self) -> Option<String> {
        self.username.read().await.clone()
    }

    pub async fn set_username(&self, username: Option<String>) {
        *self.username.write().await = username;
    }

    pub async fn set_backend(&self, cloud_base_url: String) {
        *self.cloud_base_url.write().await = cloud_base_url;
        *self.sqlite_cache_path.write().await = FIXED_SQLITE_CACHE_PATH.to_string();
    }

    /// Updates close behavior used by the synchronous window event callback.
    pub fn set_close_behavior(&self, close_behavior: CloseBehavior) {
        *self
            .close_behavior
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = close_behavior;
    }

    /// Returns the active window close behavior used by the main window event handler.
    pub fn close_behavior(&self) -> CloseBehavior {
        *self
            .close_behavior
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    pub async fn upsert_collection(&self, record: CollectionRecord) {
        self.collections
            .write()
            .await
            .insert(record.id.clone(), record);
    }

    pub async fn list_collections(&self) -> Vec<CollectionSummary> {
        self.collections
            .read()
            .await
            .values()
            .map(|record| CollectionSummary {
                id: record.id.clone(),
                name: record.name.clone(),
                synced_items: record.synced_items,
            })
            .collect()
    }

    pub async fn record_sync_results(&self, applied_by_space: &BTreeMap<uuid::Uuid, u64>) {
        let mut collections = self.collections.write().await;
        for (space_id, amount) in applied_by_space {
            if *amount == 0 {
                continue;
            }
            if let Some(collection) = collections.get_mut(&space_id.to_string()) {
                collection.synced_items = collection.synced_items.saturating_add(*amount);
            }
        }
    }

    pub async fn snapshot(&self) -> DashboardSnapshot {
        let runner = self.bridge.lock().await.clone();
        let running = match runner {
            Some(runner) => runner.is_running().await,
            None => false,
        };
        let collections = self.collections.read().await;
        let synced_items_total = collections.values().map(|c| c.synced_items).sum();

        DashboardSnapshot {
            has_access_token: self.access_token.read().await.is_some(),
            server: LocalServerStatus {
                running,
                bind_addr: "127.0.0.1:8181".to_string(),
            },
            collections_total: collections.len(),
            synced_items_total,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn sync_counts_are_recorded_for_the_matching_collection() {
        let state = DesktopState::new("https://api.kamori.app", FIXED_SQLITE_CACHE_PATH);
        let first_id = uuid::Uuid::new_v4();
        let second_id = uuid::Uuid::new_v4();
        for (id, name) in [(first_id, "First"), (second_id, "Second")] {
            state
                .upsert_collection(CollectionRecord {
                    id: id.to_string(),
                    name: name.to_string(),
                    cmk: [0_u8; 32],
                    key_epoch: 1,
                    sync_start_seq: 0,
                    synced_items: 0,
                })
                .await;
        }

        state
            .record_sync_results(&BTreeMap::from([(second_id, 4)]))
            .await;

        let collections = state.list_collections().await;
        assert_eq!(
            collections
                .iter()
                .find(|collection| collection.id == first_id.to_string())
                .expect("first collection")
                .synced_items,
            0
        );
        assert_eq!(
            collections
                .iter()
                .find(|collection| collection.id == second_id.to_string())
                .expect("second collection")
                .synced_items,
            4
        );
    }
}
