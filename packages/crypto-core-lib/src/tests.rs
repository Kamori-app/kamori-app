use super::*;

#[test]
fn aes_gcm_roundtrip() {
    let key = CryptoEngine::random_symmetric_key();
    let nonce = CryptoEngine::random_nonce_12();
    let payload = b"hello world";
    let enc = CryptoEngine::encrypt_payload(
        CipherAlgorithm::Aes256Gcm,
        &key.0,
        &nonce,
        payload,
        Some(b"aad"),
    )
    .unwrap();
    let dec = CryptoEngine::decrypt_payload(&enc, &key.0, Some(b"aad")).unwrap();
    assert_eq!(dec, payload);
}

#[test]
fn xchacha_roundtrip() {
    let key = CryptoEngine::random_symmetric_key();
    let nonce = CryptoEngine::random_nonce_24();
    let payload = b"secret payload";
    let enc = CryptoEngine::encrypt_payload(
        CipherAlgorithm::XChaCha20Poly1305,
        &key.0,
        &nonce,
        payload,
        None,
    )
    .unwrap();
    let dec = CryptoEngine::decrypt_payload(&enc, &key.0, None).unwrap();
    assert_eq!(dec, payload);
}

#[test]
fn group_key_wrap_unwrap() {
    let cmk = CryptoEngine::random_symmetric_key();
    let recipient = CryptoEngine::generate_x25519_keypair();
    let wrapped = CryptoEngine::encrypt_group_key_for_peer(&cmk.0, &recipient.public_key).unwrap();
    let unwrapped =
        CryptoEngine::decrypt_group_key_from_peer(&wrapped, &recipient.private_key).unwrap();
    assert_eq!(unwrapped, cmk.0);
}
