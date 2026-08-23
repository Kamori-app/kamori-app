use rand_core::{OsRng, RngCore};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::{CipherAlgorithm, CryptoEngine, EncryptedPayload, secret_vault};

use super::{
    state::{
        MOBILE_ACCOUNT_MASTER_KEY, MOBILE_BRIDGE_RUNTIME, MOBILE_COLLECTION_KEYS,
        MOBILE_DEVICE_SECRETS, MOBILE_REFRESH_TOKEN, MOBILE_SYNC_STARTS,
        set_mobile_refresh_token,
    },
    transport::{encode_msgpack, get_msgpack_with_auth_refresh, post_msgpack_with_auth_refresh},
    types::{
        MobileCreateInviteCodeRequest, MobileCreateInviteCodeResponse, MobileDeviceKeyPackage,
        MobileIssuedInviteCode, MobileListSpaceDevicesResponse, MobileListSpaceMembersResponse,
        MobileListSpacesResponse, MobileMemberRecoveryKeyPackage, MobilePutDeviceKeyPackageRequest,
        MobilePutRecoveryKeyPackageRequest, MobileRedeemInviteCodeRequest,
        MobileRedeemInviteCodeResponse, MobileRedeemedInvite, MobileRotateSpaceKeyRequest,
        MobileRotateSpaceKeyResponse, MobileStoredResponse,
    },
};

fn normalize_invite_code(invite_code: &str) -> String {
    invite_code
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .map(|ch| ch.to_ascii_uppercase())
        .collect()
}

const INVITE_HASH_DOMAIN: &[u8] = b"kamori:invite:lookup:v1";
const INVITE_KEY_DOMAIN: &[u8] = b"kamori:invite:key:v1";

fn domain_separated_hash(domain: &[u8], normalized_invite_code: &str) -> [u8; 32] {
    let code_bytes = normalized_invite_code.as_bytes();
    let mut input = Vec::with_capacity(domain.len() + 1 + code_bytes.len());
    input.extend_from_slice(domain);
    input.push(0);
    input.extend_from_slice(code_bytes);

    let digest = Sha256::digest(&input);
    let mut out = [0u8; 32];
    out.copy_from_slice(&digest[..32]);
    out
}

fn generate_invite_code() -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
    let mut rng = OsRng;
    let mut raw = String::with_capacity(16);
    for _ in 0..16 {
        raw.push(ALPHABET[(rng.next_u32() as usize) % ALPHABET.len()] as char);
    }
    format!(
        "{}-{}-{}-{}",
        &raw[0..4],
        &raw[4..8],
        &raw[8..12],
        &raw[12..16]
    )
}

fn hash_invite_code(invite_code: &str) -> Result<[u8; 32], String> {
    let normalized = normalize_invite_code(invite_code);
    if normalized.len() != 16 {
        return Err("invite code format is invalid".to_string());
    }

    Ok(domain_separated_hash(INVITE_HASH_DOMAIN, &normalized))
}

fn derive_invite_key(invite_code: &str) -> Result<[u8; 32], String> {
    let normalized = normalize_invite_code(invite_code);
    if normalized.len() != 16 {
        return Err("invite code format is invalid".to_string());
    }

    Ok(domain_separated_hash(INVITE_KEY_DOMAIN, &normalized))
}

fn wrap_collection_key_with_invite_code(
    collection_key: [u8; 32],
    invite_code: &str,
) -> Result<Vec<u8>, String> {
    let derived_key = derive_invite_key(invite_code)?;
    let nonce = CryptoEngine::random_nonce_24();
    let encrypted = CryptoEngine::encrypt_payload(
        CipherAlgorithm::XChaCha20Poly1305,
        &derived_key,
        &nonce,
        collection_key.as_slice(),
        None,
    )
    .map_err(|error| format!("failed to encrypt collection key for invite: {error}"))?;
    let mut payload = Vec::with_capacity(24 + encrypted.ciphertext.len());
    payload.extend_from_slice(&nonce);
    payload.extend_from_slice(&encrypted.ciphertext);
    Ok(payload)
}

fn unwrap_collection_key_with_invite_code(
    encrypted_group_key: &[u8],
    invite_code: &str,
) -> Result<[u8; 32], String> {
    if encrypted_group_key.len() < 24 {
        return Err("encrypted invite payload is malformed".to_string());
    }

    let derived_key = derive_invite_key(invite_code)?;
    let nonce = encrypted_group_key[..24].to_vec();
    let ciphertext = encrypted_group_key[24..].to_vec();
    let encrypted_payload = EncryptedPayload {
        algorithm: CipherAlgorithm::XChaCha20Poly1305,
        nonce,
        ciphertext,
    };

    let decrypted = CryptoEngine::decrypt_payload(&encrypted_payload, &derived_key, None)
        .map_err(|error| format!("failed to decrypt invite payload: {error}"))?;

    decrypted
        .try_into()
        .map_err(|_| "decrypted invite payload has invalid key size".to_string())
}

pub(super) async fn mobile_create_invite_code_impl(
    collection_id: String,
    collection_key: [u8; 32],
    ttl_minutes: u32,
) -> Result<MobileIssuedInviteCode, String> {
    let _runtime_lease = super::state::MOBILE_RUNTIME_LEASE.lock().await;
    if !(15..=7 * 24 * 60).contains(&ttl_minutes) {
        return Err("ttl_minutes must be between 15 and 10080".to_string());
    }

    let space_id = Uuid::parse_str(&collection_id)
        .map_err(|error| format!("invalid collection id: {error}"))?;
    let (expected_key_epoch, registered_key) = MOBILE_COLLECTION_KEYS
        .lock()
        .await
        .get(&collection_id)
        .copied()
        .ok_or_else(|| "security-space key is not registered".to_string())?;
    if registered_key != collection_key {
        return Err("the supplied security-space key is stale".to_string());
    }

    let runner = super::bridge::mobile_runner().await?;
    runner.sync_once().await.map_err(|error| error.to_string())?;
    set_mobile_refresh_token(runner.current_refresh_token().await).await;
    let base_space_seq = runner
        .space_cursor(space_id)
        .await
        .map_err(|error| error.to_string())?;
    let spaces: MobileListSpacesResponse = get_current_session("/spaces").await?;
    let space = spaces
        .spaces
        .into_iter()
        .find(|space| space.space_id == space_id)
        .ok_or_else(|| "security space is no longer accessible".to_string())?;
    if space.role != "owner" {
        return Err("only the space owner can create membership invites".to_string());
    }
    if space.key_epoch != expected_key_epoch {
        return Err("security-space key epoch changed; provision keys again".to_string());
    }

    let members: MobileListSpaceMembersResponse =
        get_current_session(&format!("/spaces/{space_id}/members")).await?;
    let devices: MobileListSpaceDevicesResponse =
        get_current_session(&format!("/spaces/{space_id}/devices")).await?;
    let new_key_epoch = expected_key_epoch
        .checked_add(1)
        .ok_or_else(|| "security-space key epoch overflow".to_string())?;
    let new_space_key = CryptoEngine::random_symmetric_key().0;
    let remaining_device_packages = devices
        .devices
        .into_iter()
        .filter(|device| device.active)
        .map(|device| {
            let public_key: [u8; 32] = device
                .hpke_public_key
                .try_into()
                .map_err(|_| "member device HPKE public key is invalid".to_string())?;
            let encrypted =
                CryptoEngine::encrypt_group_key_for_peer(&new_space_key, &public_key)
                    .map_err(|error| error.to_string())?;
            Ok(MobileDeviceKeyPackage {
                device_id: device.device_id,
                key_epoch: new_key_epoch,
                encrypted_key_package: rmp_serde::to_vec_named(&encrypted)
                    .map_err(|error| error.to_string())?,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;

    #[derive(serde::Deserialize)]
    struct AccountPublicKeyBundleV2 {
        version: u8,
        #[serde(with = "serde_bytes")]
        account_recovery_public_key: Vec<u8>,
    }
    let remaining_recovery_packages = members
        .members
        .into_iter()
        .map(|member| {
            let bundle: AccountPublicKeyBundleV2 =
                rmp_serde::from_slice(&member.public_key_bundle)
                    .map_err(|_| format!("member {} has an invalid recovery key", member.username))?;
            if bundle.version != 2 {
                return Err(format!(
                    "member {} has an unsupported recovery key",
                    member.username
                ));
            }
            let public_key: [u8; 32] = bundle
                .account_recovery_public_key
                .try_into()
                .map_err(|_| format!("member {} has an invalid recovery key", member.username))?;
            let encrypted =
                CryptoEngine::encrypt_group_key_for_peer(&new_space_key, &public_key)
                    .map_err(|error| error.to_string())?;
            Ok(MobileMemberRecoveryKeyPackage {
                user_id: member.user_id,
                key_epoch: new_key_epoch,
                encrypted_key_package: rmp_serde::to_vec_named(&encrypted)
                    .map_err(|error| error.to_string())?,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let snapshots = runner
        .build_rotation_snapshots(
            space_id,
            new_key_epoch,
            new_space_key,
            base_space_seq,
        )
        .await
        .map_err(|error| error.to_string())?;
    let snapshot_streams = snapshots
        .iter()
        .map(|snapshot| snapshot.stream_id)
        .collect::<std::collections::HashSet<_>>();
    let quarantined_streams = runner
        .quarantined_stream_ids(space_id, base_space_seq)
        .await
        .map_err(|error| error.to_string())?
        .into_iter()
        .filter(|stream_id| !snapshot_streams.contains(stream_id))
        .collect();
    let metadata = secret_vault::decrypt(&collection_key, &space.encrypted_metadata)
        .map_err(|error| format!("failed to decrypt space metadata: {error}"))?;
    let rotation_id = Uuid::new_v4();
    let rotated: MobileRotateSpaceKeyResponse = post_current_session(
        &format!("/spaces/{space_id}/rotate-key"),
        &MobileRotateSpaceKeyRequest {
            rotation_id,
            expected_key_epoch,
            new_key_epoch,
            base_space_seq,
            new_encrypted_metadata: secret_vault::encrypt(&new_space_key, &metadata)
                .map_err(|error| format!("failed to encrypt space metadata: {error}"))?,
            remaining_device_packages,
            remaining_recovery_packages,
            snapshots,
            quarantined_streams,
        },
    )
    .await?;
    if !rotated.rotated || rotated.key_epoch != new_key_epoch {
        return Err("server returned an invalid key-rotation result".to_string());
    }
    MOBILE_COLLECTION_KEYS
        .lock()
        .await
        .insert(collection_id, (new_key_epoch, new_space_key));

    let invite_code = generate_invite_code();
    let invite_code_hash = hash_invite_code(&invite_code)?;
    let created: MobileCreateInviteCodeResponse = post_current_session(
        "/invite-codes",
        &MobileCreateInviteCodeRequest {
            space_id,
            rotation_id,
            role: "editor".to_string(),
            invite_code_hash: invite_code_hash.to_vec(),
            encrypted_key_package: wrap_collection_key_with_invite_code(
                new_space_key,
                &invite_code,
            )?,
            encrypted_note: None,
            ttl_minutes,
        },
    )
    .await?;
    let _ = created.id;
    Ok(MobileIssuedInviteCode {
        code: invite_code,
        ttl_minutes,
        key_epoch: new_key_epoch,
        current_state_start_seq: base_space_seq,
        collection_key: new_space_key,
    })
}

pub(super) async fn mobile_redeem_invite_code_impl(
    invite_code: String,
) -> Result<MobileRedeemedInvite, String> {
    let config = {
        let runtime = MOBILE_BRIDGE_RUNTIME.lock().await;
        runtime
            .last_config
            .clone()
            .ok_or_else(|| "mobile sync runtime has not been configured yet".to_string())?
    };

    let invite_code_hash = hash_invite_code(&invite_code)?;

    let request = MobileRedeemInviteCodeRequest {
        invite_code_hash: invite_code_hash.to_vec(),
    };
    let refresh_token = MOBILE_REFRESH_TOKEN.lock().await.clone();
    let body = encode_msgpack(&request)?;
    let (redeemed, rotated_tokens): (MobileRedeemInviteCodeResponse, Option<(String, String)>) =
        post_msgpack_with_auth_refresh(
            &config.cloud_base_url,
            "/invite-codes/redeem",
            body,
            &config.access_token,
            refresh_token.as_deref(),
        )
        .await?;

    if let Some((new_access_token, new_refresh_token)) = rotated_tokens {
        {
            let mut runtime = MOBILE_BRIDGE_RUNTIME.lock().await;
            if let Some(last_config) = runtime.last_config.as_mut() {
                last_config.access_token = new_access_token;
            }
        }
        set_mobile_refresh_token(Some(new_refresh_token)).await;
    }
    let collection_key = unwrap_collection_key_with_invite_code(
        &redeemed.encrypted_key_package,
        &invite_code,
    )?;
    let collection_id = redeemed.space_id.to_string();

    let device = MOBILE_DEVICE_SECRETS
        .lock()
        .await
        .clone()
        .ok_or_else(|| "mobile device has not been provisioned".to_string())?;
    let account_master_key = MOBILE_ACCOUNT_MASTER_KEY
        .lock()
        .await
        .as_ref()
        .copied()
        .ok_or_else(|| "account master key is not loaded".to_string())?;
    let device_id = Uuid::parse_str(&device.device_id)
        .map_err(|error| format!("invalid mobile device id: {error}"))?;
    let encrypted_device_key = CryptoEngine::encrypt_group_key_for_peer(
        &collection_key,
        &device.hpke_public_key,
    )
    .map_err(|error| error.to_string())?;
    let device_request = MobilePutDeviceKeyPackageRequest {
        package: MobileDeviceKeyPackage {
            device_id,
            key_epoch: redeemed.key_epoch,
            encrypted_key_package: rmp_serde::to_vec_named(&encrypted_device_key)
                .map_err(|error| error.to_string())?,
        },
    };
    post_current_session::<_, MobileStoredResponse>(
        &format!("/spaces/{}/device-key-packages", redeemed.space_id),
        &device_request,
    )
    .await?;
    let recovery_request = MobilePutRecoveryKeyPackageRequest {
        key_epoch: redeemed.key_epoch,
        encrypted_key_package: super::devices::wrap_recovery_space_key(
            &account_master_key,
            &collection_key,
        )?,
    };
    post_current_session::<_, MobileStoredResponse>(
        &format!("/spaces/{}/recovery-key-package", redeemed.space_id),
        &recovery_request,
    )
    .await?;

    {
        let mut keys = MOBILE_COLLECTION_KEYS.lock().await;
        keys.insert(
            collection_id.clone(),
            (redeemed.key_epoch, collection_key),
        );
    }
    MOBILE_SYNC_STARTS
        .lock()
        .await
        .insert(
            collection_id.clone(),
            redeemed
                .history_start_seq
                .max(redeemed.current_state_start_seq),
        );

    Ok(MobileRedeemedInvite {
        collection_id,
        role: redeemed.role,
        key_epoch: redeemed.key_epoch,
        history_start_seq: redeemed.history_start_seq,
        current_state_start_seq: redeemed.current_state_start_seq,
        collection_key,
    })
}

async fn post_current_session<T, R>(path: &str, payload: &T) -> Result<R, String>
where
    T: serde::Serialize,
    R: serde::de::DeserializeOwned,
{
    let config = MOBILE_BRIDGE_RUNTIME
        .lock()
        .await
        .last_config
        .clone()
        .ok_or_else(|| "mobile sync has not been configured yet".to_string())?;
    let refresh_token = MOBILE_REFRESH_TOKEN.lock().await.clone();
    let (response, rotated) = post_msgpack_with_auth_refresh(
        &config.cloud_base_url,
        path,
        encode_msgpack(payload)?,
        &config.access_token,
        refresh_token.as_deref(),
    )
    .await?;
    if let Some((new_access_token, new_refresh_token)) = rotated {
        if let Some(last_config) = MOBILE_BRIDGE_RUNTIME.lock().await.last_config.as_mut() {
            last_config.access_token = new_access_token;
        }
        set_mobile_refresh_token(Some(new_refresh_token)).await;
    }
    Ok(response)
}

async fn get_current_session<R>(path: &str) -> Result<R, String>
where
    R: serde::de::DeserializeOwned,
{
    let config = MOBILE_BRIDGE_RUNTIME
        .lock()
        .await
        .last_config
        .clone()
        .ok_or_else(|| "mobile sync has not been configured yet".to_string())?;
    let refresh_token = MOBILE_REFRESH_TOKEN.lock().await.clone();
    let (response, rotated) = get_msgpack_with_auth_refresh(
        &config.cloud_base_url,
        path,
        &config.access_token,
        refresh_token.as_deref(),
    )
    .await?;
    if let Some((new_access_token, new_refresh_token)) = rotated {
        if let Some(last_config) = MOBILE_BRIDGE_RUNTIME.lock().await.last_config.as_mut() {
            last_config.access_token = new_access_token;
        }
        set_mobile_refresh_token(Some(new_refresh_token)).await;
    }
    Ok(response)
}
