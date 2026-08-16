use aes_gcm::{
    Aes256Gcm, KeyInit,
    aead::{Aead, Payload},
};
use anyhow::{Result, anyhow};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use hkdf::Hkdf;
use hpke::{
    Deserializable, Kem as KemTrait, OpModeR, OpModeS, Serializable, aead::ChaCha20Poly1305,
    kdf::HkdfSha256, kem::X25519HkdfSha256, setup_receiver, setup_sender,
};
use rand_core::{OsRng, RngCore};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use x25519_dalek::{PublicKey, StaticSecret};
use zeroize::{Zeroize, ZeroizeOnDrop};

#[derive(Debug, thiserror::Error)]
pub enum CryptoError {
    #[error("invalid key length")]
    InvalidKeyLength,
    #[error("invalid nonce length")]
    InvalidNonceLength,
    #[error("encryption failed")]
    EncryptionFailed,
    #[error("decryption failed")]
    DecryptionFailed,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CipherAlgorithm {
    Aes256Gcm,
    XChaCha20Poly1305,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct EncryptedPayload {
    pub algorithm: CipherAlgorithm,
    pub nonce: Vec<u8>,
    pub ciphertext: Vec<u8>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct EncryptedGroupKey {
    pub version: u8,
    pub encapsulated_key: Vec<u8>,
    pub ciphertext: Vec<u8>,
}

#[derive(Clone, Debug, Zeroize, ZeroizeOnDrop)]
pub struct SymmetricKey(pub [u8; 32]);

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Keypair {
    pub private_key: [u8; 32],
    pub public_key: [u8; 32],
}

pub struct CryptoEngine;

impl CryptoEngine {
    pub fn generate_x25519_keypair() -> Keypair {
        let mut secret_bytes = [0u8; 32];
        OsRng.fill_bytes(&mut secret_bytes);
        let secret = StaticSecret::from(secret_bytes);
        let public = PublicKey::from(&secret);
        Keypair {
            private_key: secret.to_bytes(),
            public_key: public.to_bytes(),
        }
    }

    pub fn derive_shared_secret(private_key: &[u8; 32], peer_public_key: &[u8; 32]) -> [u8; 32] {
        let sk = StaticSecret::from(*private_key);
        let pk = PublicKey::from(*peer_public_key);
        sk.diffie_hellman(&pk).to_bytes()
    }

    pub fn hkdf_sha256(ikm: &[u8], salt: Option<&[u8]>, info: &[u8]) -> [u8; 32] {
        let hk = Hkdf::<Sha256>::new(salt, ikm);
        let mut okm = [0u8; 32];
        hk.expand(info, &mut okm).expect("hkdf expand");
        okm
    }

    pub fn random_symmetric_key() -> SymmetricKey {
        let mut key = [0u8; 32];
        OsRng.fill_bytes(&mut key);
        SymmetricKey(key)
    }

    pub fn random_nonce_12() -> [u8; 12] {
        let mut nonce = [0u8; 12];
        OsRng.fill_bytes(&mut nonce);
        nonce
    }

    pub fn random_nonce_24() -> [u8; 24] {
        let mut nonce = [0u8; 24];
        OsRng.fill_bytes(&mut nonce);
        nonce
    }

    pub fn encrypt_payload(
        algorithm: CipherAlgorithm,
        key: &[u8; 32],
        nonce: &[u8],
        plaintext: &[u8],
        aad: Option<&[u8]>,
    ) -> Result<EncryptedPayload> {
        match algorithm {
            CipherAlgorithm::Aes256Gcm => {
                if nonce.len() != 12 {
                    return Err(CryptoError::InvalidNonceLength.into());
                }
                let cipher =
                    Aes256Gcm::new_from_slice(key).map_err(|_| CryptoError::InvalidKeyLength)?;
                let nonce =
                    aes_gcm::Nonce::try_from(nonce).map_err(|_| CryptoError::InvalidNonceLength)?;
                let ct = cipher
                    .encrypt(
                        &nonce,
                        Payload {
                            msg: plaintext,
                            aad: aad.unwrap_or(&[]),
                        },
                    )
                    .map_err(|_| CryptoError::EncryptionFailed)?;
                Ok(EncryptedPayload {
                    algorithm,
                    nonce: nonce.to_vec(),
                    ciphertext: ct,
                })
            }
            CipherAlgorithm::XChaCha20Poly1305 => {
                if nonce.len() != 24 {
                    return Err(CryptoError::InvalidNonceLength.into());
                }
                let cipher = XChaCha20Poly1305::new_from_slice(key)
                    .map_err(|_| CryptoError::InvalidKeyLength)?;
                let nonce = XNonce::try_from(nonce).map_err(|_| CryptoError::InvalidNonceLength)?;
                let ct = cipher
                    .encrypt(
                        &nonce,
                        Payload {
                            msg: plaintext,
                            aad: aad.unwrap_or(&[]),
                        },
                    )
                    .map_err(|_| CryptoError::EncryptionFailed)?;
                Ok(EncryptedPayload {
                    algorithm,
                    nonce: nonce.to_vec(),
                    ciphertext: ct,
                })
            }
        }
    }

    pub fn decrypt_payload(
        encrypted: &EncryptedPayload,
        key: &[u8; 32],
        aad: Option<&[u8]>,
    ) -> Result<Vec<u8>> {
        match encrypted.algorithm {
            CipherAlgorithm::Aes256Gcm => {
                if encrypted.nonce.len() != 12 {
                    return Err(CryptoError::InvalidNonceLength.into());
                }
                let cipher =
                    Aes256Gcm::new_from_slice(key).map_err(|_| CryptoError::InvalidKeyLength)?;
                let nonce = aes_gcm::Nonce::try_from(encrypted.nonce.as_slice())
                    .map_err(|_| CryptoError::InvalidNonceLength)?;
                let pt = cipher
                    .decrypt(
                        &nonce,
                        Payload {
                            msg: encrypted.ciphertext.as_ref(),
                            aad: aad.unwrap_or(&[]),
                        },
                    )
                    .map_err(|_| CryptoError::DecryptionFailed)?;
                Ok(pt)
            }
            CipherAlgorithm::XChaCha20Poly1305 => {
                if encrypted.nonce.len() != 24 {
                    return Err(CryptoError::InvalidNonceLength.into());
                }
                let cipher = XChaCha20Poly1305::new_from_slice(key)
                    .map_err(|_| CryptoError::InvalidKeyLength)?;
                let nonce = XNonce::try_from(encrypted.nonce.as_slice())
                    .map_err(|_| CryptoError::InvalidNonceLength)?;
                let pt = cipher
                    .decrypt(
                        &nonce,
                        Payload {
                            msg: encrypted.ciphertext.as_ref(),
                            aad: aad.unwrap_or(&[]),
                        },
                    )
                    .map_err(|_| CryptoError::DecryptionFailed)?;
                Ok(pt)
            }
        }
    }

    pub fn encrypt_group_key_for_peer(
        cmk: &[u8; 32],
        peer_public_key: &[u8; 32],
    ) -> Result<EncryptedGroupKey> {
        type Kem = X25519HkdfSha256;
        type Kdf = HkdfSha256;
        type Aead = ChaCha20Poly1305;

        const INFO: &[u8] = b"kamori.hpke.space-key.v1";
        const AAD: &[u8] = b"kamori.security-space-key-package.v1";
        let public_key = <Kem as KemTrait>::PublicKey::from_bytes(peer_public_key)
            .map_err(|_| CryptoError::InvalidKeyLength)?;
        let (encapsulated_key, mut context) =
            setup_sender::<Aead, Kdf, Kem>(&OpModeS::Base, &public_key, INFO)
                .map_err(|_| CryptoError::EncryptionFailed)?;
        let ciphertext = context
            .seal(cmk, AAD)
            .map_err(|_| CryptoError::EncryptionFailed)?;
        Ok(EncryptedGroupKey {
            version: 1,
            encapsulated_key: encapsulated_key.to_bytes().as_slice().to_vec(),
            ciphertext,
        })
    }

    pub fn decrypt_group_key_from_peer(
        encrypted: &EncryptedGroupKey,
        recipient_private_key: &[u8; 32],
    ) -> Result<[u8; 32]> {
        type Kem = X25519HkdfSha256;
        type Kdf = HkdfSha256;
        type Aead = ChaCha20Poly1305;

        const INFO: &[u8] = b"kamori.hpke.space-key.v1";
        const AAD: &[u8] = b"kamori.security-space-key-package.v1";
        if encrypted.version != 1 {
            return Err(anyhow!("unsupported HPKE key package version"));
        }
        let private_key = <Kem as KemTrait>::PrivateKey::from_bytes(recipient_private_key)
            .map_err(|_| CryptoError::InvalidKeyLength)?;
        let encapsulated_key =
            <Kem as KemTrait>::EncappedKey::from_bytes(encrypted.encapsulated_key.as_slice())
                .map_err(|_| CryptoError::DecryptionFailed)?;
        let mut context =
            setup_receiver::<Aead, Kdf, Kem>(&OpModeR::Base, &private_key, &encapsulated_key, INFO)
                .map_err(|_| CryptoError::DecryptionFailed)?;
        let plaintext = context
            .open(&encrypted.ciphertext, AAD)
            .map_err(|_| CryptoError::DecryptionFailed)?;
        if plaintext.len() != 32 {
            return Err(anyhow!("invalid decrypted group key length"));
        }
        let mut out = [0u8; 32];
        out.copy_from_slice(&plaintext);
        Ok(out)
    }
}
