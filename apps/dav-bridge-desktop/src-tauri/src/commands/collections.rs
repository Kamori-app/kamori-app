//! Server-backed security-space commands surfaced as collections in the desktop UI.

use crypto_core_lib::{CryptoEngine, secret_vault};
use serde::Serialize;
use tauri::State;
use uuid::Uuid;

use crate::{
    models::CollectionSummary,
    state::{CollectionRecord, DesktopState},
};

use super::common::{
    MSGPACK_CONTENT_TYPE, encode_msgpack, endpoint, load_account_master_key_secure,
    load_or_create_device_secrets, to_ui_error,
};

#[derive(Serialize)]
struct SpaceMetadata<'a> {
    version: u8,
    kind: &'static str,
    name: &'a str,
}

#[derive(Serialize)]
struct DeviceKeyPackage {
    device_id: Uuid,
    key_epoch: u32,
    #[serde(with = "serde_bytes")]
    encrypted_key_package: Vec<u8>,
}

#[derive(Serialize)]
struct CreateSpaceRequest {
    space_id: Uuid,
    #[serde(with = "serde_bytes")]
    encrypted_metadata: Vec<u8>,
    device_key_packages: Vec<DeviceKeyPackage>,
    #[serde(with = "serde_bytes")]
    encrypted_recovery_key_package: Vec<u8>,
}

/// Creates an encrypted security space and installs its key in the local bridge.
#[tauri::command]
pub async fn create_collection(
    state: State<'_, DesktopState>,
    name: String,
) -> Result<CollectionSummary, String> {
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err("collection name is required".to_string());
    }
    if name.chars().count() > 120 {
        return Err("collection name must not exceed 120 characters".to_string());
    }

    let access_token = state.require_access_token().await.map_err(to_ui_error)?;
    let base = state.cloud_base_url().await;
    let username = state
        .username()
        .await
        .ok_or_else(|| "sign in again before creating a collection".to_string())?;
    let device = load_or_create_device_secrets(&base, &username)?;
    let space_id = Uuid::new_v4();
    let space_key = CryptoEngine::random_symmetric_key().0;
    let metadata = rmp_serde::to_vec_named(&SpaceMetadata {
        version: 1,
        kind: "pim",
        name: &name,
    })
    .map_err(to_ui_error)?;
    let encrypted_metadata = secret_vault::encrypt(&space_key, &metadata).map_err(to_ui_error)?;
    let encrypted_key =
        CryptoEngine::encrypt_group_key_for_peer(&space_key, &device.hpke_public_key)
            .map_err(to_ui_error)?;
    let encrypted_key_package = rmp_serde::to_vec_named(&encrypted_key).map_err(to_ui_error)?;
    let master_key = load_account_master_key_secure(&base, &username)?;
    let encrypted_recovery_key_package =
        secret_vault::encrypt(&master_key, &space_key).map_err(to_ui_error)?;
    let body = encode_msgpack(&CreateSpaceRequest {
        space_id,
        encrypted_metadata,
        device_key_packages: vec![DeviceKeyPackage {
            device_id: device.device_id,
            key_epoch: 1,
            encrypted_key_package,
        }],
        encrypted_recovery_key_package,
    })?;
    reqwest::Client::new()
        .post(endpoint(&base, "/spaces"))
        .bearer_auth(access_token)
        .header(reqwest::header::CONTENT_TYPE, MSGPACK_CONTENT_TYPE)
        .header(reqwest::header::ACCEPT, MSGPACK_CONTENT_TYPE)
        .body(body)
        .send()
        .await
        .map_err(to_ui_error)?
        .error_for_status()
        .map_err(to_ui_error)?;

    let id = space_id.to_string();
    let record = CollectionRecord {
        id: id.clone(),
        name: name.clone(),
        cmk: space_key,
        key_epoch: 1,
        sync_start_seq: 0,
        synced_items: 0,
    };
    state.upsert_collection(record.clone()).await;

    if let Some(runner) = state.bridge.lock().await.as_ref().cloned() {
        runner
            .register_collection_key_epoch(record.id.clone(), record.key_epoch, record.cmk)
            .await;
    }

    Ok(CollectionSummary {
        id,
        name,
        synced_items: 0,
    })
}

/// Lists all security spaces whose key is available on this device.
#[tauri::command]
pub async fn list_collections(
    state: State<'_, DesktopState>,
) -> Result<Vec<CollectionSummary>, String> {
    Ok(state.list_collections().await)
}
