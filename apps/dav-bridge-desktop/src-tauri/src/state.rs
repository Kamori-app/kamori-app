use crate::models::{CollectionSummary, DashboardSnapshot, LocalServerStatus};
use anyhow::{Result, anyhow};
use crypto_core_lib::local_bridge_runner::{LocalBridgeRunner, LocalDeviceIdentity};
use std::{
    collections::HashMap,
    sync::{Arc, RwLock as StdRwLock},
};
use tokio::sync::{Mutex, RwLock};
use tokio::task::JoinHandle;

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
    close_behavior: Arc<StdRwLock<CloseBehavior>>,
    tray_icon_enabled: Arc<StdRwLock<bool>>,
}

#[derive(Clone)]
pub struct CollectionRecord {
    pub id: String,
    pub name: String,
    pub cmk: [u8; 32],
    pub key_epoch: u32,
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
            close_behavior: Arc::new(StdRwLock::new(CloseBehavior::Quit)),
            tray_icon_enabled: Arc::new(StdRwLock::new(false)),
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
        *self.access_token.write().await = token;
    }

    pub async fn refresh_token(&self) -> Option<String> {
        self.refresh_token.read().await.clone()
    }

    pub async fn set_refresh_token(&self, token: Option<String>) {
        *self.refresh_token.write().await = token;
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

    /// Updates close behavior and tray icon preferences atomically for window handling.
    pub fn set_window_preferences(&self, close_behavior: CloseBehavior, tray_icon_enabled: bool) {
        *self
            .close_behavior
            .write()
            .expect("window close behavior lock poisoned") = close_behavior;
        *self
            .tray_icon_enabled
            .write()
            .expect("tray icon preference lock poisoned") = tray_icon_enabled;
    }

    /// Returns the active window close behavior used by the main window event handler.
    pub fn close_behavior(&self) -> CloseBehavior {
        *self
            .close_behavior
            .read()
            .expect("window close behavior lock poisoned")
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

    pub async fn increment_synced_items(&self, amount: u64) {
        if amount == 0 {
            return;
        }
        let mut collections = self.collections.write().await;
        if let Some(first) = collections.values_mut().next() {
            first.synced_items = first.synced_items.saturating_add(amount);
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
