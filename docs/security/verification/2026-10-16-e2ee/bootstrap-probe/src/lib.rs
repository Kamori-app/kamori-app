//! Design experiment only. This is not the Kamori bootstrap implementation.
#[cfg(test)]
mod tests {
    use hpke::{
        OpModeR, OpModeS, PskBundle,
        aead::ChaCha20Poly1305,
        kdf::HkdfSha256,
        kem::{Kem as _, X25519HkdfSha256},
        setup_receiver, setup_sender,
    };

    type Kem = X25519HkdfSha256;
    type Kdf = HkdfSha256;
    type Aead = ChaCha20Poly1305;
    const INFO: &[u8] = b"kamori.design-probe.bootstrap.v2";
    const AAD: &[u8] = b"trusted-origins/flow/nonce/recipient/account/checkpoints";
    const ID: &[u8] = b"one-use-local-pairing";
    const SECRET: &[u8; 32] = &[0x41; 32];

    #[test]
    fn base_accepts_attacker_selected_root_without_recipient_substitution() {
        let (sk, pk) = Kem::gen_keypair();
        let attacker_root = [0x99; 32];
        let (enc, mut sender) = setup_sender::<Aead, Kdf, Kem>(&OpModeS::Base, &pk, INFO).unwrap();
        let ct = sender.seal(&attacker_root, AAD).unwrap();
        let mut receiver =
            setup_receiver::<Aead, Kdf, Kem>(&OpModeR::Base, &sk, &enc, INFO).unwrap();
        assert_eq!(receiver.open(&ct, AAD).unwrap(), attacker_root);
    }

    #[test]
    fn matching_out_of_band_psk_accepts_honest_return() {
        let (sk, pk) = Kem::gen_keypair();
        let root = [0x22; 32];
        let psk = PskBundle::new(SECRET, ID).unwrap();
        let (enc, mut sender) =
            setup_sender::<Aead, Kdf, Kem>(&OpModeS::Psk(psk), &pk, INFO).unwrap();
        let ct = sender.seal(&root, AAD).unwrap();
        let mut receiver =
            setup_receiver::<Aead, Kdf, Kem>(&OpModeR::Psk(psk), &sk, &enc, INFO).unwrap();
        assert_eq!(receiver.open(&ct, AAD).unwrap(), root);
    }

    #[test]
    fn mandatory_psk_rejects_api_replacement_using_public_key_only() {
        let (sk, pk) = Kem::gen_keypair();
        let (enc, mut attacker) =
            setup_sender::<Aead, Kdf, Kem>(&OpModeS::Base, &pk, INFO).unwrap();
        let ct = attacker.seal(&[0x99; 32], AAD).unwrap();
        let psk = PskBundle::new(SECRET, ID).unwrap();
        let mut receiver =
            setup_receiver::<Aead, Kdf, Kem>(&OpModeR::Psk(psk), &sk, &enc, INFO).unwrap();
        assert!(receiver.open(&ct, AAD).is_err());
    }

    #[test]
    fn mandatory_psk_rejects_wrong_pairing_secret() {
        let (sk, pk) = Kem::gen_keypair();
        let attacker_secret = [0x98; 32];
        let wrong = PskBundle::new(&attacker_secret, ID).unwrap();
        let (enc, mut attacker) =
            setup_sender::<Aead, Kdf, Kem>(&OpModeS::Psk(wrong), &pk, INFO).unwrap();
        let ct = attacker.seal(&[0x99; 32], AAD).unwrap();
        let psk = PskBundle::new(SECRET, ID).unwrap();
        let mut receiver =
            setup_receiver::<Aead, Kdf, Kem>(&OpModeR::Psk(psk), &sk, &enc, INFO).unwrap();
        assert!(receiver.open(&ct, AAD).is_err());
    }

    #[test]
    fn authenticated_return_rejects_different_public_transcript() {
        let (sk, pk) = Kem::gen_keypair();
        let psk = PskBundle::new(SECRET, ID).unwrap();
        let (enc, mut sender) =
            setup_sender::<Aead, Kdf, Kem>(&OpModeS::Psk(psk), &pk, INFO).unwrap();
        let ct = sender.seal(&[0x22; 32], AAD).unwrap();
        let mut receiver =
            setup_receiver::<Aead, Kdf, Kem>(&OpModeR::Psk(psk), &sk, &enc, INFO).unwrap();
        assert!(receiver.open(&ct, b"different-flow-or-account").is_err());
    }

    #[test]
    fn library_accepts_empty_bundle_so_application_needs_length_guard() {
        assert!(PskBundle::new(&[], &[]).is_ok());
        assert!(PskBundle::new(SECRET, &[]).is_err());
    }
}
