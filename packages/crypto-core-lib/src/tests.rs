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

#[test]
fn device_bootstrap_is_bound_to_recipient_and_authorization_flow() {
    let master_key = [19_u8; 32];
    let recipient = CryptoEngine::generate_x25519_keypair();
    let other_recipient = CryptoEngine::generate_x25519_keypair();
    let flow_id = uuid::Uuid::new_v4();
    let wrapped =
        CryptoEngine::encrypt_device_bootstrap(&master_key, &recipient.public_key, flow_id)
            .expect("wrap device bootstrap");

    assert_eq!(
        CryptoEngine::decrypt_device_bootstrap(&wrapped, &recipient.private_key, flow_id)
            .expect("unwrap device bootstrap"),
        master_key
    );
    assert!(
        CryptoEngine::decrypt_device_bootstrap(
            &wrapped,
            &recipient.private_key,
            uuid::Uuid::new_v4(),
        )
        .is_err()
    );
    assert!(
        CryptoEngine::decrypt_device_bootstrap(&wrapped, &other_recipient.private_key, flow_id,)
            .is_err()
    );
}

#[test]
fn account_recovery_identity_is_deterministic_and_unwraps_space_keys() {
    let master_key = [41_u8; 32];
    let first = CryptoEngine::derive_account_recovery_keypair(&master_key);
    let second = CryptoEngine::derive_account_recovery_keypair(&master_key);
    assert_eq!(first.private_key, second.private_key);
    assert_eq!(first.public_key, second.public_key);
    assert_ne!(
        first.public_key,
        CryptoEngine::derive_account_recovery_keypair(&[42_u8; 32]).public_key
    );

    let space_key = CryptoEngine::random_symmetric_key();
    let wrapped =
        CryptoEngine::encrypt_group_key_for_peer(&space_key.0, &first.public_key).unwrap();
    assert_eq!(
        CryptoEngine::decrypt_group_key_from_peer(&wrapped, &first.private_key).unwrap(),
        space_key.0
    );
}
