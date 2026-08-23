use crate::{CipherAlgorithm, CryptoEngine, EncryptedGroupKey, EncryptedPayload, Keypair};

pub(super) fn generate_x25519_keypair_impl() -> Keypair {
    CryptoEngine::generate_x25519_keypair()
}

pub(super) fn encrypt_payload_impl(
    algorithm: CipherAlgorithm,
    key: [u8; 32],
    nonce: Vec<u8>,
    plaintext: Vec<u8>,
    aad: Vec<u8>,
) -> Result<EncryptedPayload, String> {
    let aad_opt = if aad.is_empty() {
        None
    } else {
        Some(aad.as_slice())
    };
    CryptoEngine::encrypt_payload(algorithm, &key, &nonce, &plaintext, aad_opt)
        .map_err(|error| error.to_string())
}

pub(super) fn decrypt_payload_impl(
    encrypted: EncryptedPayload,
    key: [u8; 32],
    aad: Vec<u8>,
) -> Result<Vec<u8>, String> {
    let aad_opt = if aad.is_empty() {
        None
    } else {
        Some(aad.as_slice())
    };
    CryptoEngine::decrypt_payload(&encrypted, &key, aad_opt).map_err(|error| error.to_string())
}

pub(super) fn encrypt_group_key_for_peer_impl(
    cmk: [u8; 32],
    peer_public_key: [u8; 32],
) -> Result<EncryptedGroupKey, String> {
    CryptoEngine::encrypt_group_key_for_peer(&cmk, &peer_public_key)
        .map_err(|error| error.to_string())
}

pub(super) fn decrypt_group_key_from_peer_impl(
    encrypted: EncryptedGroupKey,
    recipient_private_key: [u8; 32],
) -> Result<[u8; 32], String> {
    CryptoEngine::decrypt_group_key_from_peer(&encrypted, &recipient_private_key)
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exported_crypto_errors_do_not_panic_the_mobile_runtime() {
        let invalid_encrypt = encrypt_payload_impl(
            CipherAlgorithm::XChaCha20Poly1305,
            [0; 32],
            vec![0; 12],
            b"payload".to_vec(),
            Vec::new(),
        );
        assert_eq!(invalid_encrypt.unwrap_err(), "invalid nonce length");

        let invalid_decrypt = decrypt_payload_impl(
            EncryptedPayload {
                algorithm: CipherAlgorithm::Aes256Gcm,
                nonce: vec![0; 12],
                ciphertext: vec![0; 16],
            },
            [0; 32],
            Vec::new(),
        );
        assert_eq!(invalid_decrypt.unwrap_err(), "decryption failed");
    }
}
