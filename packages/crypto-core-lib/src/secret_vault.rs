//! Portable encryption for account-local secrets and opaque metadata.

use crate::{CipherAlgorithm, CryptoEngine, EncryptedPayload};

const AAD: &[u8] = b"kamori.secret-vault.v1";

pub fn encrypt(key: &[u8; 32], plaintext: &[u8]) -> anyhow::Result<Vec<u8>> {
    let encrypted = CryptoEngine::encrypt_payload(
        CipherAlgorithm::XChaCha20Poly1305,
        key,
        &CryptoEngine::random_nonce_24(),
        plaintext,
        Some(AAD),
    )?;
    Ok(rmp_serde::to_vec_named(&encrypted)?)
}

pub fn decrypt(key: &[u8; 32], encrypted: &[u8]) -> anyhow::Result<Vec<u8>> {
    let encrypted: EncryptedPayload = rmp_serde::from_slice(encrypted)?;
    CryptoEngine::decrypt_payload(&encrypted, key, Some(AAD))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secret_vault_rejects_wrong_key() {
        let encrypted = encrypt(&[1; 32], b"metadata").expect("encrypt");
        assert_eq!(decrypt(&[1; 32], &encrypted).expect("decrypt"), b"metadata");
        assert!(decrypt(&[2; 32], &encrypted).is_err());
    }
}
