//! Local bridge lifecycle and sync commands.
use crate::{
    models::{DavCollectionEndpoint, DavConnectionInfo, LocalServerStatus},
    state::DesktopState,
};
use crypto_core_lib::local_bridge_runner::{LocalBridgeConfig, LocalBridgeRunner};
use tauri::State;
use tokio::time::{Duration, MissedTickBehavior};
use tracing::warn;

use super::common::{
    ensure_parent_dir, load_or_create_sqlite_key, load_refresh_token_secure,
    rotate_dav_credentials as rotate_dav_credentials_secure, store_refresh_token_secure,
    to_ui_error, with_sqlite_key,
};

async fn sync_runtime_refresh_token(
    state: &DesktopState,
    runner: &LocalBridgeRunner,
) -> Result<(), String> {
    let Some(refresh_token) = runner.current_refresh_token().await else {
        return Ok(());
    };
    let cloud_base_url = state.cloud_base_url().await;
    store_refresh_token_secure(&cloud_base_url, &refresh_token)?;
    state.set_refresh_token(Some(refresh_token)).await;
    Ok(())
}

async fn stop_sync_task(state: &DesktopState) {
    if let Some(handle) = state.sync_task.lock().await.take() {
        handle.abort();
        let _ = handle.await;
    }
}

async fn start_sync_task(state: &DesktopState, runner: LocalBridgeRunner) {
    stop_sync_task(state).await;
    let task_state = state.clone();
    let handle = tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(60));
        interval.set_missed_tick_behavior(MissedTickBehavior::Skip);
        loop {
            interval.tick().await;
            match runner.sync_once().await {
                Ok(applied) => {
                    task_state.increment_synced_items(applied).await;
                    if let Err(error) = sync_runtime_refresh_token(&task_state, &runner).await {
                        warn!(%error, "failed to persist rotated desktop refresh token");
                    }
                }
                Err(error) => warn!(?error, "desktop background sync failed"),
            }
        }
    });
    *state.sync_task.lock().await = Some(handle);
}

/// Starts the local bridge server on localhost:8181.
#[tauri::command]
pub async fn start_local_server(
    state: State<'_, DesktopState>,
) -> Result<LocalServerStatus, String> {
    if let Some(runner) = state.bridge.lock().await.clone()
        && runner.is_running().await
    {
        return Ok(LocalServerStatus {
            running: true,
            bind_addr: runner.bind_addr().to_string(),
        });
    }

    let cloud_base_url = state.cloud_base_url().await;
    let sqlite_path = state.sqlite_cache_path().await;
    ensure_parent_dir(&sqlite_path)?;
    let sqlite_key = load_or_create_sqlite_key(&sqlite_path)?;
    let access_token = state.require_access_token().await.map_err(to_ui_error)?;
    let refresh_token = match state.refresh_token().await {
        some @ Some(_) => some,
        None => {
            let loaded = load_refresh_token_secure(&cloud_base_url)?;
            if let Some(token) = loaded.clone() {
                state.set_refresh_token(Some(token)).await;
            }
            loaded
        }
    };

    let mut config = LocalBridgeConfig::new(sqlite_path, cloud_base_url, access_token);
    if let Some(refresh_token) = refresh_token {
        config = config.with_refresh_token(refresh_token);
    }
    let identity = state
        .device_identity
        .read()
        .await
        .clone()
        .ok_or_else(|| "sign in again to approve this desktop device".to_string())?;
    let (dav_username, dav_password) = state
        .dav_credentials
        .read()
        .await
        .clone()
        .ok_or_else(|| "dedicated DAV credentials are unavailable".to_string())?;
    config = config
        .with_device_identity(identity)
        .with_dav_credentials(dav_username, dav_password);
    let runner =
        LocalBridgeRunner::new(with_sqlite_key(config, sqlite_key)).map_err(to_ui_error)?;

    for collection in state.collections.read().await.values() {
        runner
            .register_collection_key_epoch(
                collection.id.clone(),
                collection.key_epoch,
                collection.cmk,
            )
            .await;
    }

    runner.start().await.map_err(to_ui_error)?;
    sync_runtime_refresh_token(state.inner(), &runner).await?;
    *state.bridge.lock().await = Some(runner.clone());
    start_sync_task(state.inner(), runner.clone()).await;

    Ok(LocalServerStatus {
        running: true,
        bind_addr: runner.bind_addr().to_string(),
    })
}

/// Stops the local bridge server if it is running.
#[tauri::command]
pub async fn stop_local_server(
    state: State<'_, DesktopState>,
) -> Result<LocalServerStatus, String> {
    stop_sync_task(state.inner()).await;
    let mut guard = state.bridge.lock().await;
    if let Some(runner) = guard.take() {
        sync_runtime_refresh_token(state.inner(), &runner).await?;
        runner.stop().await.map_err(to_ui_error)?;
    }

    Ok(LocalServerStatus {
        running: false,
        bind_addr: "127.0.0.1:8181".to_string(),
    })
}

/// Returns current local server status.
#[tauri::command]
pub async fn local_server_status(
    state: State<'_, DesktopState>,
) -> Result<LocalServerStatus, String> {
    let runner = state.bridge.lock().await.clone();
    if let Some(runner) = runner.as_ref() {
        sync_runtime_refresh_token(state.inner(), runner).await?;
    }

    Ok(LocalServerStatus {
        running: match runner {
            Some(runner) => runner.is_running().await,
            None => false,
        },
        bind_addr: "127.0.0.1:8181".to_string(),
    })
}

#[tauri::command]
pub async fn dav_connection_info(
    state: State<'_, DesktopState>,
) -> Result<DavConnectionInfo, String> {
    state.require_access_token().await.map_err(to_ui_error)?;
    let (username, password) = state
        .dav_credentials
        .read()
        .await
        .clone()
        .ok_or_else(|| "dedicated DAV credentials are unavailable".to_string())?;
    let mut collections = state
        .collections
        .read()
        .await
        .values()
        .map(|collection| DavCollectionEndpoint {
            collection_id: collection.id.clone(),
            name: collection.name.clone(),
            calendar_url: format!("http://127.0.0.1:8181/caldav/{}/", collection.id),
            address_book_url: format!("http://127.0.0.1:8181/carddav/{}/", collection.id),
        })
        .collect::<Vec<_>>();
    collections.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(DavConnectionInfo {
        bind_addr: "127.0.0.1:8181".to_string(),
        username,
        password,
        collections,
    })
}

#[tauri::command]
pub async fn rotate_dav_credentials(
    state: State<'_, DesktopState>,
) -> Result<DavConnectionInfo, String> {
    if let Some(runner) = state.bridge.lock().await.clone()
        && runner.is_running().await
    {
        return Err("stop the local bridge before rotating its credentials".to_string());
    }
    let username = state
        .username()
        .await
        .ok_or_else(|| "sign in again before rotating DAV credentials".to_string())?;
    let cloud_base_url = state.cloud_base_url().await;
    let credentials = rotate_dav_credentials_secure(&cloud_base_url, &username)?;
    *state.dav_credentials.write().await = Some(credentials);
    dav_connection_info(state).await
}

/// Runs one synchronization cycle from cloud events into local cache.
#[tauri::command]
pub async fn sync_now(state: State<'_, DesktopState>) -> Result<u64, String> {
    let runner = state
        .bridge
        .lock()
        .await
        .clone()
        .ok_or_else(|| "local server is not running".to_string())?;
    if !runner.is_running().await {
        return Err("local server is not running".to_string());
    }

    let applied = runner.sync_once().await.map_err(to_ui_error)?;
    sync_runtime_refresh_token(state.inner(), &runner).await?;
    state.increment_synced_items(applied).await;
    Ok(applied)
}
