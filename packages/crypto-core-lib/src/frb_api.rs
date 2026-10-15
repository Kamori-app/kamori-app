use opaque_ke::Ristretto255;
use opaque_ke::argon2::Argon2;
use opaque_ke::ciphersuite::CipherSuite;
use opaque_ke::key_exchange::tripledh::TripleDh;
use sha2_opaque::Sha512;

use crate::{CipherAlgorithm, CryptoEngine, EncryptedGroupKey, EncryptedPayload, Keypair};

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
    MobileDeviceSecrets, MobileIssuedInviteCode, MobileLoginResult, MobilePimDraft, MobilePimItem,
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
pub fn derive_account_recovery_keypair(master_key: [u8; 32]) -> Keypair {
    CryptoEngine::derive_account_recovery_keypair(&master_key)
}

#[flutter_rust_bridge::frb]
pub fn encrypt_payload(
    algorithm: CipherAlgorithm,
    key: [u8; 32],
    nonce: Vec<u8>,
    plaintext: Vec<u8>,
    aad: Vec<u8>,
) -> Result<EncryptedPayload, String> {
    crypto_exports::encrypt_payload_impl(algorithm, key, nonce, plaintext, aad)
}

#[flutter_rust_bridge::frb]
pub fn decrypt_payload(
    encrypted: EncryptedPayload,
    key: [u8; 32],
    aad: Vec<u8>,
) -> Result<Vec<u8>, String> {
    crypto_exports::decrypt_payload_impl(encrypted, key, aad)
}

#[flutter_rust_bridge::frb]
pub fn encrypt_group_key_for_peer(
    cmk: [u8; 32],
    peer_public_key: [u8; 32],
) -> Result<EncryptedGroupKey, String> {
    crypto_exports::encrypt_group_key_for_peer_impl(cmk, peer_public_key)
}

#[flutter_rust_bridge::frb]
pub fn decrypt_group_key_from_peer(
    encrypted: EncryptedGroupKey,
    recipient_private_key: [u8; 32],
) -> Result<[u8; 32], String> {
    crypto_exports::decrypt_group_key_from_peer_impl(encrypted, recipient_private_key)
}

#[flutter_rust_bridge::frb]
pub async fn mobile_password_login(
    session: crate::frb_generated::RustOpaque<MobileSessionLease>,
    cloud_base_url: String,
    username: String,
    password: String,
    totp_code: Option<String>,
) -> Result<MobileLoginResult, String> {
    with_mobile_session(
        session,
        auth::mobile_password_login_impl(cloud_base_url, username, password, totp_code),
    )
    .await?
}

/// Generates a mobile device identity before any server-side enrollment so
/// Flutter can persist the private keys transactionally in platform storage.
#[flutter_rust_bridge::frb]
pub fn mobile_generate_device_secrets() -> MobileDeviceSecrets {
    devices::generate_device_secrets()
}

#[flutter_rust_bridge::frb]
pub async fn mobile_provision_device_and_spaces(
    session: crate::frb_generated::RustOpaque<MobileSessionLease>,
    cloud_base_url: String,
    access_token: String,
    account_master_key: [u8; 32],
    platform: String,
    device_enrollment_token: Option<String>,
    existing_device: Option<MobileDeviceSecrets>,
) -> Result<MobileProvisionResult, String> {
    with_mobile_session(
        session,
        devices::mobile_provision_device_and_spaces_impl(
            cloud_base_url,
            access_token,
            account_master_key,
            platform,
            device_enrollment_token,
            existing_device,
        ),
    )
    .await?
}

#[flutter_rust_bridge::frb]
pub async fn mobile_create_collection(
    session: crate::frb_generated::RustOpaque<MobileSessionLease>,
    name: String,
) -> Result<types::MobileCollection, String> {
    with_mobile_session(session, devices::mobile_create_collection_impl(name)).await?
}

#[flutter_rust_bridge::frb]
pub async fn mobile_move_collection_to_trash(
    session: crate::frb_generated::RustOpaque<MobileSessionLease>,
    collection_id: String,
) -> Result<(), String> {
    with_mobile_session(
        session,
        devices::mobile_move_collection_to_trash_impl(collection_id),
    )
    .await?
}

#[flutter_rust_bridge::frb]
pub async fn mobile_import_refresh_token(
    session: crate::frb_generated::RustOpaque<MobileSessionLease>,
    refresh_token: String,
    rotation_request_id: String,
) -> Result<(), String> {
    with_mobile_session(
        session,
        auth::mobile_import_refresh_token_impl(refresh_token, rotation_request_id),
    )
    .await?
}

#[flutter_rust_bridge::frb]
pub async fn mobile_export_refresh_token(
    session: crate::frb_generated::RustOpaque<MobileSessionLease>,
) -> Result<Option<String>, String> {
    with_mobile_session(session, auth::mobile_export_refresh_token_impl()).await
}

#[flutter_rust_bridge::frb]
pub async fn mobile_export_refresh_rotation_request_id(
    session: crate::frb_generated::RustOpaque<MobileSessionLease>,
) -> Result<Option<String>, String> {
    with_mobile_session(
        session,
        auth::mobile_export_refresh_rotation_request_id_impl(),
    )
    .await
}

#[flutter_rust_bridge::frb]
pub async fn mobile_clear_refresh_token(
    session: crate::frb_generated::RustOpaque<MobileSessionLease>,
) -> Result<(), String> {
    with_mobile_session(session, auth::mobile_clear_refresh_token_impl()).await
}

#[flutter_rust_bridge::frb]
pub async fn mobile_revoke_refresh_session(
    session: crate::frb_generated::RustOpaque<MobileSessionLease>,
    cloud_base_url: String,
    refresh_token: String,
) -> Result<bool, String> {
    with_mobile_session(
        session,
        auth::mobile_revoke_refresh_session_impl(cloud_base_url, refresh_token),
    )
    .await?
}

#[flutter_rust_bridge::frb]
pub async fn mobile_create_invite_code(
    session: crate::frb_generated::RustOpaque<MobileSessionLease>,
    collection_id: String,
    collection_key: [u8; 32],
    ttl_minutes: u32,
) -> Result<MobileIssuedInviteCode, String> {
    with_mobile_session(
        session,
        invites::mobile_create_invite_code_impl(collection_id, collection_key, ttl_minutes),
    )
    .await?
}

#[flutter_rust_bridge::frb]
pub async fn mobile_redeem_invite_code(
    session: crate::frb_generated::RustOpaque<MobileSessionLease>,
    invite_code: String,
) -> Result<MobileRedeemedInvite, String> {
    with_mobile_session(
        session,
        invites::mobile_redeem_invite_code_impl(invite_code),
    )
    .await?
}

#[flutter_rust_bridge::frb]
pub async fn mobile_configure_sync(
    session: crate::frb_generated::RustOpaque<MobileSessionLease>,
    cloud_base_url: String,
    sqlite_path: String,
    access_token: String,
    sqlite_key: [u8; 32],
    device: Option<MobileDeviceSecrets>,
) -> Result<(), String> {
    with_mobile_session(
        session,
        bridge::mobile_configure_sync_impl(
            cloud_base_url,
            sqlite_path,
            access_token,
            sqlite_key,
            device,
        ),
    )
    .await?
}

#[flutter_rust_bridge::frb]
pub async fn mobile_sync_now(
    session: crate::frb_generated::RustOpaque<MobileSessionLease>,
) -> Result<u64, String> {
    with_mobile_session(session, bridge::mobile_sync_now_impl()).await?
}

#[flutter_rust_bridge::frb]
pub async fn mobile_list_pim_items(
    session: crate::frb_generated::RustOpaque<MobileSessionLease>,
) -> Result<Vec<MobilePimItem>, String> {
    with_mobile_session(session, bridge::mobile_list_pim_items_impl()).await?
}

#[flutter_rust_bridge::frb]
pub async fn mobile_upsert_pim_item(
    session: crate::frb_generated::RustOpaque<MobileSessionLease>,
    draft: MobilePimDraft,
) -> Result<MobilePimItem, String> {
    with_mobile_session(session, bridge::mobile_upsert_pim_item_impl(draft)).await?
}

#[flutter_rust_bridge::frb]
pub async fn mobile_delete_pim_item(
    session: crate::frb_generated::RustOpaque<MobileSessionLease>,
    space_id: String,
    resource_id: String,
    projection_id: String,
    head_operation_id: String,
    resource_kind: String,
) -> Result<(), String> {
    with_mobile_session(
        session,
        bridge::mobile_delete_pim_item_impl(
            space_id,
            resource_id,
            projection_id,
            head_operation_id,
            resource_kind,
        ),
    )
    .await?
}

#[flutter_rust_bridge::frb]
pub async fn mobile_register_collection_key(
    session: crate::frb_generated::RustOpaque<MobileSessionLease>,
    collection_id: String,
    key_epoch: u32,
    sync_start_seq: u64,
    cmk: [u8; 32],
) -> Result<(), String> {
    with_mobile_session(
        session,
        bridge::mobile_register_collection_key_impl(collection_id, key_epoch, sync_start_seq, cmk),
    )
    .await?
}

#[flutter_rust_bridge::frb]
pub async fn mobile_unregister_collection_key(
    session: crate::frb_generated::RustOpaque<MobileSessionLease>,
    collection_id: String,
) -> Result<(), String> {
    with_mobile_session(
        session,
        bridge::mobile_unregister_collection_key_impl(collection_id),
    )
    .await?
}

/// Coordinates the process-global mobile runtime and platform-store operations
/// across foreground and Workmanager engines. Independent of runtime/DB locks.
#[flutter_rust_bridge::frb(opaque)]
pub struct MobileSessionLease {
    guard: tokio::sync::OnceCell<tokio::sync::OwnedMutexGuard<()>>,
}

/// Attach Dart's NativeFinalizer before beginning an async acquisition. Returning
/// a newly acquired opaque handle asynchronously could orphan it if its target
/// engine exits before receiving the result.
#[flutter_rust_bridge::frb(sync)]
pub fn mobile_new_session_lease() -> crate::frb_generated::RustOpaque<MobileSessionLease> {
    crate::frb_generated::RustOpaque::new(MobileSessionLease {
        guard: tokio::sync::OnceCell::new(),
    })
}

#[flutter_rust_bridge::frb]
pub async fn mobile_acquire_session_lease(
    session: crate::frb_generated::RustOpaque<MobileSessionLease>,
) {
    ensure_mobile_session(&session).await;
}

async fn ensure_mobile_session(session: &MobileSessionLease) {
    session
        .guard
        .get_or_init(|| async { MOBILE_SESSION_COORDINATOR.clone().lock_owned().await })
        .await;
}

static MOBILE_SESSION_COORDINATOR: once_cell::sync::Lazy<std::sync::Arc<tokio::sync::Mutex<()>>> =
    once_cell::sync::Lazy::new(|| std::sync::Arc::new(tokio::sync::Mutex::new(())));

/// Keep native ownership until the operation resolves. Dart's finalizer may
/// drop its own Arc while a headless engine is shutting down; that must not
/// unlock the process-global runtime underneath this native future.
async fn with_mobile_session<T>(
    session: crate::frb_generated::RustOpaque<MobileSessionLease>,
    operation: impl std::future::Future<Output = T>,
) -> Result<T, String> {
    if session.guard.get().is_none() {
        return Err("Mobile session lease has not been acquired".to_string());
    }
    let result = operation.await;
    drop(session);
    Ok(result)
}

#[cfg(test)]
mod session_lease_tests {
    use super::*;
    use std::future::Future;
    use std::task::{Context, Poll, Waker};

    async fn acquire() -> crate::frb_generated::RustOpaque<MobileSessionLease> {
        let session = mobile_new_session_lease();
        mobile_acquire_session_lease(session.clone()).await;
        session
    }

    #[tokio::test]
    async fn unacquired_handle_rejects_operation_without_polling_it() {
        let session = mobile_new_session_lease();
        let ran = std::sync::atomic::AtomicBool::new(false);
        let result = with_mobile_session(session.clone(), async {
            ran.store(true, std::sync::atomic::Ordering::SeqCst);
        })
        .await;
        assert!(result.unwrap_err().contains("has not been acquired"));
        assert!(!ran.load(std::sync::atomic::Ordering::SeqCst));
        assert!(mobile_export_refresh_token(session).await.is_err());
    }

    #[tokio::test]
    async fn session_lease_excludes_concurrent_engines_and_releases_on_drop() {
        let owner = acquire().await;
        let mut next = std::pin::pin!(acquire());
        let mut cx = Context::from_waker(Waker::noop());
        assert!(matches!(next.as_mut().poll(&mut cx), Poll::Pending));
        drop(owner);
        let next_owner = next.await;
        drop(next_owner);
        drop(acquire().await);
    }

    #[tokio::test]
    async fn in_flight_native_operation_retains_lease_after_dart_owner_disposal() {
        let dart_owner = acquire().await;
        let (finish, completion) = tokio::sync::oneshot::channel::<()>();
        let mut native_operation = std::pin::pin!(with_mobile_session(dart_owner.clone(), async {
            completion.await.unwrap()
        },));
        let mut cx = Context::from_waker(Waker::noop());
        assert!(matches!(
            native_operation.as_mut().poll(&mut cx),
            Poll::Pending
        ));
        let mut next = std::pin::pin!(acquire());
        drop(dart_owner);
        assert!(matches!(next.as_mut().poll(&mut cx), Poll::Pending));
        finish.send(()).unwrap();
        native_operation.await.unwrap();
        drop(next.await);
    }

    #[tokio::test]
    async fn abandoned_waiting_engine_does_not_orphan_the_next_lease() {
        let current = acquire().await;
        let dart_owner = mobile_new_session_lease();
        let mut waiting_native_call =
            std::pin::pin!(mobile_acquire_session_lease(dart_owner.clone()));
        let mut cx = Context::from_waker(Waker::noop());
        assert!(matches!(
            waiting_native_call.as_mut().poll(&mut cx),
            Poll::Pending
        ));
        drop(dart_owner); // Engine finalization while its native acquisition waits.
        drop(current);
        waiting_native_call.await; // Returns unit, never an undeliverable new Arc.
        drop(acquire().await);
    }
}
