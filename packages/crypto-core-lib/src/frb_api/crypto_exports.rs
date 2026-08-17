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
) -> EncryptedPayload {
    let aad_opt = if aad.is_empty() {
        None
    } else {
        Some(aad.as_slice())
    };
    CryptoEngine::encrypt_payload(algorithm, &key, &nonce, &plaintext, aad_opt).expect("encrypt")
}

pub(super) fn decrypt_payload_impl(
    encrypted: EncryptedPayload,
    key: [u8; 32],
    aad: Vec<u8>,
) -> Vec<u8> {
    let aad_opt = if aad.is_empty() {
        None
    } else {
        Some(aad.as_slice())
    };
    CryptoEngine::decrypt_payload(&encrypted, &key, aad_opt).expect("decrypt")
}

pub(super) fn encrypt_group_key_for_peer_impl(
    cmk: [u8; 32],
    peer_public_key: [u8; 32],
) -> EncryptedGroupKey {
    CryptoEngine::encrypt_group_key_for_peer(&cmk, &peer_public_key).expect("encrypt")
}

pub(super) fn decrypt_group_key_from_peer_impl(
    encrypted: EncryptedGroupKey,
    recipient_private_key: [u8; 32],
) -> [u8; 32] {
    CryptoEngine::decrypt_group_key_from_peer(&encrypted, &recipient_private_key).expect("decrypt")
}
