use rand_core::{OsRng, RngCore};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::{CipherAlgorithm, CryptoEngine, EncryptedPayload, secret_vault};

use super::{
    state::{
        MOBILE_ACCOUNT_MASTER_KEY, MOBILE_BRIDGE_RUNTIME, MOBILE_COLLECTION_KEYS,
        MOBILE_DEVICE_SECRETS, MOBILE_REFRESH_TOKEN, set_mobile_refresh_token,
    },
    transport::{encode_msgpack, post_msgpack_with_auth_refresh},
    types::{
        MobileCreateInviteCodeRequest, MobileCreateInviteCodeResponse, MobileIssuedInviteCode,
        MobileDeviceKeyPackage, MobilePutDeviceKeyPackageRequest,
        MobilePutRecoveryKeyPackageRequest, MobileRedeemInviteCodeRequest,
        MobileRedeemInviteCodeResponse, MobileRedeemedInvite, MobileStoredResponse,
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
        let idx = (rng.next_u32() as usize) % ALPHABET.len();
        raw.push(ALPHABET[idx] as char);
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
    if !(15..=7 * 24 * 60).contains(&ttl_minutes) {
        return Err("ttl_minutes must be between 15 and 10080".to_string());
    }

    let config = {
        let runtime = MOBILE_BRIDGE_RUNTIME.lock().await;
        runtime
            .last_config
            .clone()
            .ok_or_else(|| "mobile sync runtime has not been configured yet".to_string())?
    };

    let collection_id = Uuid::parse_str(&collection_id)
        .map_err(|error| format!("invalid collection id: {error}"))?;
    let invite_code = generate_invite_code();
    let invite_code_hash = hash_invite_code(&invite_code)?;
    let encrypted_group_key = wrap_collection_key_with_invite_code(collection_key, &invite_code)?;

    let request = MobileCreateInviteCodeRequest {
        space_id: collection_id,
        role: "editor".to_string(),
        invite_code_hash: invite_code_hash.to_vec(),
        encrypted_key_package: encrypted_group_key,
        encrypted_note: None,
        ttl_minutes,
    };
    let refresh_token = MOBILE_REFRESH_TOKEN.lock().await.clone();
    let body = encode_msgpack(&request)?;
    let (created, rotated_tokens): (MobileCreateInviteCodeResponse, Option<(String, String)>) =
        post_msgpack_with_auth_refresh(
            &config.cloud_base_url,
            "/invite-codes",
            body,
            &config.access_token,
            refresh_token.as_deref(),
        )
        .await?;
    let _ = created.id;

    if let Some((new_access_token, new_refresh_token)) = rotated_tokens {
        {
            let mut runtime = MOBILE_BRIDGE_RUNTIME.lock().await;
            if let Some(last_config) = runtime.last_config.as_mut() {
                last_config.access_token = new_access_token;
            }
        }
        set_mobile_refresh_token(Some(new_refresh_token)).await;
    }

    Ok(MobileIssuedInviteCode {
        code: invite_code,
        ttl_minutes,
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
        encrypted_key_package: secret_vault::encrypt(&account_master_key, &collection_key)
            .map_err(|error| error.to_string())?,
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

    Ok(MobileRedeemedInvite {
        collection_id,
        role: redeemed.role,
        key_epoch: redeemed.key_epoch,
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
