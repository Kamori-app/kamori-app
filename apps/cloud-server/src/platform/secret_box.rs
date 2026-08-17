//! Small server-side envelope for secrets that must be recoverable at runtime.

use anyhow::{Context, Result, bail};
use chacha20poly1305::{
    XChaCha20Poly1305, XNonce,
    aead::{Aead, KeyInit, Payload},
};
use rand::RngExt;

const FORMAT_VERSION: u8 = 1;
const NONCE_BYTES: usize = 24;
const ADMIN_DOMAIN: &[u8] = b"kamori.admin.totp.v1";
const USER_DOMAIN: &[u8] = b"kamori.user.totp.v1";

fn aad(domain: &[u8], binding: &str) -> Vec<u8> {
    let mut value = Vec::with_capacity(domain.len() + 1 + binding.len());
    value.extend_from_slice(domain);
    value.push(0);
    value.extend_from_slice(binding.as_bytes());
    value
}

fn encrypt(key: &[u8; 32], domain: &[u8], binding: &str, plaintext: &str) -> Result<Vec<u8>> {
    let cipher = XChaCha20Poly1305::new_from_slice(key)
        .map_err(|_| anyhow::anyhow!("invalid operator TOTP KEK"))?;
    let mut nonce = [0_u8; NONCE_BYTES];
    rand::rng().fill(&mut nonce);
    let nonce_value = XNonce::from(nonce);
    let ciphertext = cipher
        .encrypt(
            &nonce_value,
            Payload {
                msg: plaintext.as_bytes(),
                aad: &aad(domain, binding),
            },
        )
        .context("encrypt operator TOTP seed")?;
    let mut envelope = Vec::with_capacity(1 + NONCE_BYTES + ciphertext.len());
    envelope.push(FORMAT_VERSION);
    envelope.extend_from_slice(&nonce);
    envelope.extend_from_slice(&ciphertext);
    Ok(envelope)
}

fn decrypt(key: &[u8; 32], domain: &[u8], binding: &str, envelope: &[u8]) -> Result<String> {
    if envelope.len() <= 1 + NONCE_BYTES || envelope[0] != FORMAT_VERSION {
        bail!("invalid operator TOTP envelope");
    }
    let cipher = XChaCha20Poly1305::new_from_slice(key)
        .map_err(|_| anyhow::anyhow!("invalid operator TOTP KEK"))?;
    let nonce_bytes: [u8; NONCE_BYTES] = envelope[1..1 + NONCE_BYTES]
        .try_into()
        .map_err(|_| anyhow::anyhow!("invalid operator TOTP nonce"))?;
    let nonce = XNonce::from(nonce_bytes);
    let plaintext = cipher
        .decrypt(
            &nonce,
            Payload {
                msg: &envelope[1 + NONCE_BYTES..],
                aad: &aad(domain, binding),
            },
        )
        .context("decrypt operator TOTP seed")?;
    String::from_utf8(plaintext).context("operator TOTP seed is not UTF-8")
}

/// Encrypts an operator TOTP seed with a deployment-owned key-encryption key.
pub fn encrypt_admin_totp(key: &[u8; 32], username: &str, plaintext: &str) -> Result<Vec<u8>> {
    encrypt(key, ADMIN_DOMAIN, username, plaintext)
}

/// Decrypts and authenticates an operator TOTP seed.
pub fn decrypt_admin_totp(key: &[u8; 32], username: &str, envelope: &[u8]) -> Result<String> {
    decrypt(key, ADMIN_DOMAIN, username, envelope)
}

/// Encrypts a consumer TOTP seed and binds it to the immutable user UUID.
pub fn encrypt_user_totp(key: &[u8; 32], user_id: &str, plaintext: &str) -> Result<Vec<u8>> {
    encrypt(key, USER_DOMAIN, user_id, plaintext)
}

/// Decrypts and authenticates a consumer TOTP seed.
pub fn decrypt_user_totp(key: &[u8; 32], user_id: &str, envelope: &[u8]) -> Result<String> {
    decrypt(key, USER_DOMAIN, user_id, envelope)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn totp_envelope_roundtrips_and_binds_username() {
        let key = [7_u8; 32];
        let envelope =
            encrypt_admin_totp(&key, "operator", "JBSWY3DPEHPK3PXP").expect("encrypt seed");
        assert_eq!(
            decrypt_admin_totp(&key, "operator", &envelope).expect("decrypt seed"),
            "JBSWY3DPEHPK3PXP"
        );
        assert!(decrypt_admin_totp(&key, "different", &envelope).is_err());
        assert!(decrypt_admin_totp(&[8_u8; 32], "operator", &envelope).is_err());
    }

    #[test]
    fn user_and_admin_domains_are_not_interchangeable() {
        let key = [9_u8; 32];
        let envelope =
            encrypt_user_totp(&key, "user-id", "JBSWY3DPEHPK3PXP").expect("encrypt user seed");
        assert_eq!(
            decrypt_user_totp(&key, "user-id", &envelope).expect("decrypt user seed"),
            "JBSWY3DPEHPK3PXP"
        );
        assert!(decrypt_admin_totp(&key, "user-id", &envelope).is_err());
    }
}
