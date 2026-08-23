use ed25519_dalek::SigningKey;
use rand_core::{OsRng, RngCore};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{CryptoEngine, EncryptedGroupKey, secret_vault};

use super::{
    state::{
        MOBILE_ACCOUNT_MASTER_KEY, MOBILE_COLLECTION_KEYS, MOBILE_DEVICE_SECRETS,
        MOBILE_REFRESH_TOKEN, MOBILE_SYNC_STARTS,
        set_mobile_refresh_token,
    },
    transport::{
        delete_msgpack_with_auth_refresh, encode_msgpack, get_msgpack_with_auth_refresh,
        post_msgpack_with_auth_refresh,
    },
    types::{
        MobileCollection, MobileCreateSpaceRequest, MobileCreateSpaceResponse,
        MobileDeviceKeyPackage, MobileDeviceSecrets, MobileListRecoveryKeyPackagesResponse,
        MobileListSpacesResponse, MobileProvisionResult, MobilePutDeviceKeyPackageRequest,
        MobilePutRecoveryKeyPackageRequest, MobileRegisterDeviceRequest,
        MobileRegisterDeviceResponse, MobileSpaceLifecycleResponse, MobileStoredResponse,
    },
};

#[derive(Deserialize, Serialize)]
struct SpaceMetadata {
    #[serde(rename = "version")]
    #[serde(default = "metadata_version")]
    _version: u8,
    #[serde(rename = "kind")]
    #[serde(default = "metadata_kind")]
    _kind: String,
    name: Option<String>,
}

const fn metadata_version() -> u8 {
    1
}

fn metadata_kind() -> String {
    "pim".to_string()
}

pub(super) fn generate_device_secrets() -> MobileDeviceSecrets {
    let hpke = CryptoEngine::generate_x25519_keypair();
    let mut signing_private_key = [0_u8; 32];
    OsRng.fill_bytes(&mut signing_private_key);
    MobileDeviceSecrets {
        device_id: Uuid::new_v4().to_string(),
        signing_private_key,
        hpke_private_key: hpke.private_key,
        hpke_public_key: hpke.public_key,
    }
}

pub(super) fn wrap_recovery_space_key(
    account_master_key: &[u8; 32],
    space_key: &[u8; 32],
) -> Result<Vec<u8>, String> {
    let identity = CryptoEngine::derive_account_recovery_keypair(account_master_key);
    let encrypted = CryptoEngine::encrypt_group_key_for_peer(space_key, &identity.public_key)
        .map_err(|error| error.to_string())?;
    rmp_serde::to_vec_named(&encrypted).map_err(|error| error.to_string())
}

fn unwrap_recovery_space_key(
    account_master_key: &[u8; 32],
    package: &[u8],
) -> Result<[u8; 32], String> {
    let encrypted: EncryptedGroupKey =
        rmp_serde::from_slice(package).map_err(|error| error.to_string())?;
    let identity = CryptoEngine::derive_account_recovery_keypair(account_master_key);
    CryptoEngine::decrypt_group_key_from_peer(&encrypted, &identity.private_key)
        .map_err(|error| error.to_string())
}

pub(super) async fn mobile_provision_device_and_spaces_impl(
    cloud_base_url: String,
    access_token: String,
    account_master_key: [u8; 32],
    platform: String,
    device_enrollment_token: Option<String>,
    existing_device: Option<MobileDeviceSecrets>,
) -> Result<MobileProvisionResult, String> {
    let cloud_base_url = crate::local_bridge_runner::normalize_cloud_base_url(&cloud_base_url)
        .map_err(|error| error.to_string())?;
    let platform = platform.trim().to_ascii_lowercase();
    if !matches!(platform.as_str(), "android" | "ios") {
        return Err("mobile platform must be android or ios".to_string());
    }
    let device = existing_device.unwrap_or_else(generate_device_secrets);
    let device_id = Uuid::parse_str(&device.device_id)
        .map_err(|error| format!("invalid mobile device id: {error}"))?;
    let signing_public_key = SigningKey::from_bytes(&device.signing_private_key)
        .verifying_key()
        .to_bytes();
    let encrypted_name = secret_vault::encrypt(
        &account_master_key,
        if platform == "ios" {
            b"iPhone or iPad"
        } else {
            b"Android device"
        },
    )
    .map_err(|error| error.to_string())?;
    let refresh_token = MOBILE_REFRESH_TOKEN.lock().await.clone();
    let mut current_access_token = access_token;
    if let Some(enrollment_token) = device_enrollment_token {
        let register_body = encode_msgpack(&MobileRegisterDeviceRequest {
            enrollment_token,
            device_id,
            signing_public_key: signing_public_key.to_vec(),
            hpke_public_key: device.hpke_public_key.to_vec(),
            encrypted_name,
            platform,
        })?;
        let (_registered, rotated): (MobileRegisterDeviceResponse, Option<(String, String)>) =
            post_msgpack_with_auth_refresh(
                &cloud_base_url,
                "/devices",
                register_body,
                &current_access_token,
                refresh_token.as_deref(),
            )
            .await?;
        if let Some((new_access_token, new_refresh_token)) = rotated {
            current_access_token = new_access_token;
            set_mobile_refresh_token(Some(new_refresh_token)).await;
        }
    }

    let refresh_token = MOBILE_REFRESH_TOKEN.lock().await.clone();
    let (spaces, rotated): (MobileListSpacesResponse, Option<(String, String)>) =
        get_msgpack_with_auth_refresh(
            &cloud_base_url,
            "/spaces",
            &current_access_token,
            refresh_token.as_deref(),
        )
        .await?;
    if let Some((new_access_token, new_refresh_token)) = rotated {
        current_access_token = new_access_token;
        set_mobile_refresh_token(Some(new_refresh_token)).await;
    }

    let refresh_token = MOBILE_REFRESH_TOKEN.lock().await.clone();
    let (recovery, rotated): (MobileListRecoveryKeyPackagesResponse, Option<(String, String)>) =
        get_msgpack_with_auth_refresh(
            &cloud_base_url,
            "/spaces/recovery-key-packages",
            &current_access_token,
            refresh_token.as_deref(),
        )
        .await?;
    if let Some((new_access_token, new_refresh_token)) = rotated {
        current_access_token = new_access_token;
        set_mobile_refresh_token(Some(new_refresh_token)).await;
    }
    let recovery_packages = recovery
        .packages
        .into_iter()
        .map(|package| {
            (
                (package.space_id, package.key_epoch),
                package.encrypted_key_package,
            )
        })
        .collect::<std::collections::HashMap<_, _>>();

    let mut collections = Vec::new();
    let mut runtime_keys = std::collections::HashMap::new();
    let mut sync_starts = std::collections::HashMap::new();
    for space in spaces.spaces {
        let device_package = space.device_key_packages.iter().find(|package| {
            package.device_id == device_id && package.key_epoch == space.key_epoch
        });
        let key = if let Some(package) = device_package {
            let encrypted: EncryptedGroupKey =
                rmp_serde::from_slice(&package.encrypted_key_package)
                    .map_err(|error| format!("invalid device key package: {error}"))?;
            CryptoEngine::decrypt_group_key_from_peer(&encrypted, &device.hpke_private_key)
                .map_err(|error| format!("failed to unwrap security-space key: {error}"))?
        } else if let Some(package) = recovery_packages.get(&(space.space_id, space.key_epoch)) {
            unwrap_recovery_space_key(&account_master_key, package)
                .map_err(|error| format!("failed to unwrap recovery key package: {error}"))?
        } else {
            continue;
        };
        if device_package.is_none() {
            let encrypted =
                CryptoEngine::encrypt_group_key_for_peer(&key, &device.hpke_public_key)
                    .map_err(|error| error.to_string())?;
            let body = encode_msgpack(&MobilePutDeviceKeyPackageRequest {
                package: MobileDeviceKeyPackage {
                    device_id,
                    key_epoch: space.key_epoch,
                    encrypted_key_package: rmp_serde::to_vec_named(&encrypted)
                        .map_err(|error| error.to_string())?,
                },
            })?;
            let refresh_token = MOBILE_REFRESH_TOKEN.lock().await.clone();
            let (_stored, rotated): (MobileStoredResponse, Option<(String, String)>) =
                post_msgpack_with_auth_refresh(
                    &cloud_base_url,
                    &format!("/spaces/{}/device-key-packages", space.space_id),
                    body,
                    &current_access_token,
                    refresh_token.as_deref(),
                )
                .await?;
            if let Some((new_access_token, new_refresh_token)) = rotated {
                current_access_token = new_access_token;
                set_mobile_refresh_token(Some(new_refresh_token)).await;
            }
        }
        let recovery_body = encode_msgpack(&MobilePutRecoveryKeyPackageRequest {
            key_epoch: space.key_epoch,
            encrypted_key_package: wrap_recovery_space_key(&account_master_key, &key)?,
        })?;
        let refresh_token = MOBILE_REFRESH_TOKEN.lock().await.clone();
        let (_stored, rotated): (MobileStoredResponse, Option<(String, String)>) =
            post_msgpack_with_auth_refresh(
                &cloud_base_url,
                &format!("/spaces/{}/recovery-key-package", space.space_id),
                recovery_body,
                &current_access_token,
                refresh_token.as_deref(),
            )
            .await?;
        if let Some((new_access_token, new_refresh_token)) = rotated {
            current_access_token = new_access_token;
            set_mobile_refresh_token(Some(new_refresh_token)).await;
        }
        let metadata = secret_vault::decrypt(&key, &space.encrypted_metadata)
            .ok()
            .and_then(|bytes| rmp_serde::from_slice::<SpaceMetadata>(&bytes).ok());
        let name = metadata
            .and_then(|metadata| metadata.name)
            .filter(|name| !name.trim().is_empty())
            .unwrap_or_else(|| format!("Space {}", &space.space_id.to_string()[..8]));
        runtime_keys.insert(space.space_id.to_string(), (space.key_epoch, key));
        sync_starts.insert(
            space.space_id.to_string(),
            space.history_start_seq.max(space.current_state_start_seq),
        );
        collections.push(MobileCollection {
            collection_id: space.space_id.to_string(),
            name,
            role: space.role,
            key_epoch: space.key_epoch,
            history_start_seq: space.history_start_seq,
            current_state_start_seq: space.current_state_start_seq,
            collection_key: key,
        });
    }
    *MOBILE_COLLECTION_KEYS.lock().await = runtime_keys;
    *MOBILE_SYNC_STARTS.lock().await = sync_starts;
    *MOBILE_DEVICE_SECRETS.lock().await = Some(device.clone());
    *MOBILE_ACCOUNT_MASTER_KEY.lock().await = Some(account_master_key);

    Ok(MobileProvisionResult {
        access_token: current_access_token,
        device,
        collections,
    })
}

pub(super) async fn mobile_create_collection_impl(
    name: String,
) -> Result<MobileCollection, String> {
    let name = name.trim().to_string();
    if name.is_empty() || name.chars().count() > 120 {
        return Err("collection name must contain 1 to 120 characters".to_string());
    }
    let config = super::state::MOBILE_BRIDGE_RUNTIME
        .lock()
        .await
        .last_config
        .clone()
        .ok_or_else(|| "mobile sync has not been configured yet".to_string())?;
    let device = MOBILE_DEVICE_SECRETS
        .lock()
        .await
        .clone()
        .ok_or_else(|| "mobile device has not been provisioned".to_string())?;
    let device_id = Uuid::parse_str(&device.device_id)
        .map_err(|error| format!("invalid mobile device id: {error}"))?;
    let space_id = Uuid::new_v4();
    let key = CryptoEngine::random_symmetric_key().0;
    let metadata = rmp_serde::to_vec_named(&SpaceMetadata {
        _version: 1,
        _kind: "pim".to_string(),
        name: Some(name.clone()),
    })
    .map_err(|error| error.to_string())?;
    let encrypted_metadata =
        secret_vault::encrypt(&key, &metadata).map_err(|error| error.to_string())?;
    let encrypted_key = CryptoEngine::encrypt_group_key_for_peer(&key, &device.hpke_public_key)
        .map_err(|error| error.to_string())?;
    let account_master_key = MOBILE_ACCOUNT_MASTER_KEY
        .lock()
        .await
        .ok_or_else(|| "account master key is not loaded".to_string())?;
    let request = MobileCreateSpaceRequest {
        space_id,
        encrypted_metadata,
        device_key_packages: vec![MobileDeviceKeyPackage {
            device_id,
            key_epoch: 1,
            encrypted_key_package: rmp_serde::to_vec_named(&encrypted_key)
                .map_err(|error| error.to_string())?,
        }],
        encrypted_recovery_key_package: wrap_recovery_space_key(&account_master_key, &key)?,
    };
    let refresh_token = MOBILE_REFRESH_TOKEN.lock().await.clone();
    let (_created, rotated): (MobileCreateSpaceResponse, Option<(String, String)>) =
        post_msgpack_with_auth_refresh(
            &config.cloud_base_url,
            "/spaces",
            encode_msgpack(&request)?,
            &config.access_token,
            refresh_token.as_deref(),
        )
        .await?;
    if let Some((new_access_token, new_refresh_token)) = rotated {
        if let Some(config) = super::state::MOBILE_BRIDGE_RUNTIME
            .lock()
            .await
            .last_config
            .as_mut()
        {
            config.access_token = new_access_token;
        }
        set_mobile_refresh_token(Some(new_refresh_token)).await;
    }
    MOBILE_COLLECTION_KEYS
        .lock()
        .await
        .insert(space_id.to_string(), (1, key));
    MOBILE_SYNC_STARTS
        .lock()
        .await
        .insert(space_id.to_string(), 0);
    Ok(MobileCollection {
        collection_id: space_id.to_string(),
        name,
        role: "owner".to_string(),
        key_epoch: 1,
        history_start_seq: 0,
        current_state_start_seq: 0,
        collection_key: key,
    })
}

pub(super) async fn mobile_move_collection_to_trash_impl(
    collection_id: String,
) -> Result<(), String> {
    let collection_id = Uuid::parse_str(&collection_id)
        .map_err(|error| format!("invalid collection id: {error}"))?;
    let config = super::state::MOBILE_BRIDGE_RUNTIME
        .lock()
        .await
        .last_config
        .clone()
        .ok_or_else(|| "mobile sync has not been configured yet".to_string())?;
    let refresh_token = MOBILE_REFRESH_TOKEN.lock().await.clone();
    let (response, rotated): (MobileSpaceLifecycleResponse, Option<(String, String)>) =
        delete_msgpack_with_auth_refresh(
            &config.cloud_base_url,
            &format!("/spaces/{collection_id}"),
            &config.access_token,
            refresh_token.as_deref(),
        )
        .await?;
    if !response.changed {
        return Err("security space was not moved to trash".to_string());
    }
    if let Some((new_access_token, new_refresh_token)) = rotated {
        if let Some(config) = super::state::MOBILE_BRIDGE_RUNTIME
            .lock()
            .await
            .last_config
            .as_mut()
        {
            config.access_token = new_access_token;
        }
        set_mobile_refresh_token(Some(new_refresh_token)).await;
    }
    MOBILE_COLLECTION_KEYS
        .lock()
        .await
        .remove(&collection_id.to_string());
    MOBILE_SYNC_STARTS
        .lock()
        .await
        .remove(&collection_id.to_string());
    Ok(())
}
