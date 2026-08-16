use opaque_ke::Ristretto255;
use opaque_ke::argon2::Argon2;
use opaque_ke::ciphersuite::CipherSuite;
use opaque_ke::key_exchange::tripledh::TripleDh;
use sha2_opaque::Sha512;

use crate::{CipherAlgorithm, EncryptedGroupKey, EncryptedPayload, Keypair};

mod auth {
    include!("frb_api/auth.rs");
}
mod bridge {
    include!("frb_api/bridge.rs");
}
mod crypto_exports {
    include!("frb_api/crypto_exports.rs");
}
mod devices {
    include!("frb_api/devices.rs");
}
mod invites {
    include!("frb_api/invites.rs");
}
mod state {
    include!("frb_api/state.rs");
}
mod transport {
    include!("frb_api/transport.rs");
}
pub mod types {
    include!("frb_api/types.rs");
}

pub use types::{
    MobileDeviceSecrets, MobileIssuedInviteCode, MobileLoginResult, MobilePimItem,
    MobileProvisionResult, MobileRedeemedInvite,
};

pub struct MobileOpaqueSuite;

impl CipherSuite for MobileOpaqueSuite {
    type OprfCs = Ristretto255;
    type KeyExchange = TripleDh<Ristretto255, Sha512>;
    type Ksf = Argon2<'static>;
}

#[flutter_rust_bridge::frb]
pub fn generate_x25519_keypair() -> Keypair {
    crypto_exports::generate_x25519_keypair_impl()
}

#[flutter_rust_bridge::frb]
pub fn encrypt_payload(
    algorithm: CipherAlgorithm,
    key: [u8; 32],
    nonce: Vec<u8>,
    plaintext: Vec<u8>,
    aad: Vec<u8>,
) -> EncryptedPayload {
    crypto_exports::encrypt_payload_impl(algorithm, key, nonce, plaintext, aad)
}

#[flutter_rust_bridge::frb]
pub fn decrypt_payload(encrypted: EncryptedPayload, key: [u8; 32], aad: Vec<u8>) -> Vec<u8> {
    crypto_exports::decrypt_payload_impl(encrypted, key, aad)
}

#[flutter_rust_bridge::frb]
pub fn encrypt_group_key_for_peer(cmk: [u8; 32], peer_public_key: [u8; 32]) -> EncryptedGroupKey {
    crypto_exports::encrypt_group_key_for_peer_impl(cmk, peer_public_key)
}

#[flutter_rust_bridge::frb]
pub fn decrypt_group_key_from_peer(
    encrypted: EncryptedGroupKey,
    recipient_private_key: [u8; 32],
) -> [u8; 32] {
    crypto_exports::decrypt_group_key_from_peer_impl(encrypted, recipient_private_key)
}

#[flutter_rust_bridge::frb]
pub async fn mobile_password_login(
    cloud_base_url: String,
    username: String,
    password: String,
    totp_code: Option<String>,
) -> Result<MobileLoginResult, String> {
    auth::mobile_password_login_impl(cloud_base_url, username, password, totp_code).await
}

#[flutter_rust_bridge::frb]
pub async fn mobile_provision_device_and_spaces(
    cloud_base_url: String,
    access_token: String,
    account_master_key: [u8; 32],
    platform: String,
    existing_device: Option<MobileDeviceSecrets>,
) -> Result<MobileProvisionResult, String> {
    devices::mobile_provision_device_and_spaces_impl(
        cloud_base_url,
        access_token,
        account_master_key,
        platform,
        existing_device,
    )
    .await
}

#[flutter_rust_bridge::frb]
pub async fn mobile_create_collection(name: String) -> Result<types::MobileCollection, String> {
    devices::mobile_create_collection_impl(name).await
}

#[flutter_rust_bridge::frb]
pub async fn mobile_move_collection_to_trash(collection_id: String) -> Result<(), String> {
    devices::mobile_move_collection_to_trash_impl(collection_id).await
}

#[flutter_rust_bridge::frb]
pub async fn mobile_import_refresh_token(refresh_token: String) -> Result<(), String> {
    auth::mobile_import_refresh_token_impl(refresh_token).await
}

#[flutter_rust_bridge::frb]
pub async fn mobile_export_refresh_token() -> Option<String> {
    auth::mobile_export_refresh_token_impl().await
}

#[flutter_rust_bridge::frb]
pub async fn mobile_clear_refresh_token() {
    auth::mobile_clear_refresh_token_impl().await
}

#[flutter_rust_bridge::frb]
pub async fn mobile_create_invite_code(
    collection_id: String,
    collection_key: [u8; 32],
    ttl_minutes: u32,
) -> Result<MobileIssuedInviteCode, String> {
    invites::mobile_create_invite_code_impl(collection_id, collection_key, ttl_minutes).await
}

#[flutter_rust_bridge::frb]
pub async fn mobile_redeem_invite_code(
    invite_code: String,
) -> Result<MobileRedeemedInvite, String> {
    invites::mobile_redeem_invite_code_impl(invite_code).await
}

#[flutter_rust_bridge::frb]
pub async fn mobile_configure_sync(
    cloud_base_url: String,
    sqlite_path: String,
    access_token: String,
    sqlite_key: [u8; 32],
    device: Option<MobileDeviceSecrets>,
) -> Result<(), String> {
    bridge::mobile_configure_sync_impl(
        cloud_base_url,
        sqlite_path,
        access_token,
        sqlite_key,
        device,
    )
    .await
}

#[flutter_rust_bridge::frb]
pub async fn mobile_sync_now() -> Result<u64, String> {
    bridge::mobile_sync_now_impl().await
}

#[flutter_rust_bridge::frb]
pub async fn mobile_list_pim_items() -> Result<Vec<MobilePimItem>, String> {
    bridge::mobile_list_pim_items_impl().await
}

#[flutter_rust_bridge::frb]
#[allow(clippy::too_many_arguments)]
pub async fn mobile_upsert_pim_item(
    space_id: String,
    resource_id: Option<String>,
    resource_kind: String,
    title: String,
    completed: bool,
    email: Option<String>,
    phone: Option<String>,
    starts_at: Option<String>,
    ends_at: Option<String>,
) -> Result<MobilePimItem, String> {
    bridge::mobile_upsert_pim_item_impl(
        space_id,
        resource_id,
        resource_kind,
        title,
        completed,
        email,
        phone,
        starts_at,
        ends_at,
    )
    .await
}

#[flutter_rust_bridge::frb]
pub async fn mobile_delete_pim_item(
    space_id: String,
    resource_id: String,
    resource_kind: String,
) -> Result<(), String> {
    bridge::mobile_delete_pim_item_impl(space_id, resource_id, resource_kind).await
}

#[flutter_rust_bridge::frb]
pub async fn mobile_register_collection_key(
    collection_id: String,
    key_epoch: u32,
    cmk: [u8; 32],
) -> Result<(), String> {
    bridge::mobile_register_collection_key_impl(collection_id, key_epoch, cmk).await
}

#[flutter_rust_bridge::frb]
pub async fn mobile_unregister_collection_key(collection_id: String) -> Result<(), String> {
    bridge::mobile_unregister_collection_key_impl(collection_id).await
}
