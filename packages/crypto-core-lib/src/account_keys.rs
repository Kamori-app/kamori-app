//! OPAQUE export-key binding for the portable account master key.

use crate::{CipherAlgorithm, CryptoEngine, EncryptedPayload};

const WRAP_INFO: &[u8] = b"kamori.opaque.account-master-key.v1";
const WRAP_AAD: &[u8] = b"kamori.account-master-key-wrap.v1";

pub fn wrap(export_key: &[u8], master_key: &[u8; 32]) -> anyhow::Result<Vec<u8>> {
    if export_key.len() < 32 {
        anyhow::bail!("OPAQUE export key is too short");
    }
    let wrapping_key = CryptoEngine::hkdf_sha256(export_key, None, WRAP_INFO);
    let encrypted = CryptoEngine::encrypt_payload(
        CipherAlgorithm::XChaCha20Poly1305,
        &wrapping_key,
        &CryptoEngine::random_nonce_24(),
        master_key,
        Some(WRAP_AAD),
    )?;
    Ok(rmp_serde::to_vec_named(&encrypted)?)
}

pub fn unwrap(export_key: &[u8], encrypted: &[u8]) -> anyhow::Result<[u8; 32]> {
    if export_key.len() < 32 {
        anyhow::bail!("OPAQUE export key is too short");
    }
    let wrapping_key = CryptoEngine::hkdf_sha256(export_key, None, WRAP_INFO);
    let encrypted: EncryptedPayload = rmp_serde::from_slice(encrypted)?;
    CryptoEngine::decrypt_payload(&encrypted, &wrapping_key, Some(WRAP_AAD))?
        .try_into()
        .map_err(|_| anyhow::anyhow!("account master key must be 32 bytes"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opaque_export_key_wrap_roundtrips() {
        let master = [3; 32];
        let wrapped = wrap(&[7; 64], &master).expect("wrap");
        assert_eq!(unwrap(&[7; 64], &wrapped).expect("unwrap"), master);
        assert!(unwrap(&[8; 64], &wrapped).is_err());
    }
}
