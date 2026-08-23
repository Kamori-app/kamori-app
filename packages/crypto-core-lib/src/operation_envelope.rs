//! Versioned signed envelope shared by every Kamori transport.

use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{CipherAlgorithm, CryptoEngine};

const SIGNING_DOMAIN: &[u8] = b"kamori.operation-envelope.v1\0";
const AAD_DOMAIN: &[u8] = b"kamori.operation-envelope-aad.v1\0";
const STREAM_KEY_DOMAIN: &[u8] = b"kamori.stream-key.v1\0";

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EnvelopeKind {
    Operation,
    Snapshot,
    Control,
}

impl EnvelopeKind {
    pub const fn as_db_value(self) -> &'static str {
        match self {
            Self::Operation => "operation",
            Self::Snapshot => "snapshot",
            Self::Control => "control",
        }
    }

    pub fn from_db_value(value: &str) -> anyhow::Result<Self> {
        match value {
            "operation" => Ok(Self::Operation),
            "snapshot" => Ok(Self::Snapshot),
            "control" => Ok(Self::Control),
            _ => anyhow::bail!("unknown operation envelope kind"),
        }
    }

    const fn tag(self) -> u8 {
        match self {
            Self::Operation => 1,
            Self::Snapshot => 2,
            Self::Control => 3,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EnvelopeCipherSuite {
    Xchacha20Poly1305,
    Aes256Gcm,
}

impl EnvelopeCipherSuite {
    pub const fn as_db_value(self) -> &'static str {
        match self {
            Self::Xchacha20Poly1305 => "xchacha20_poly1305",
            Self::Aes256Gcm => "aes256_gcm",
        }
    }

    pub fn from_db_value(value: &str) -> anyhow::Result<Self> {
        match value {
            "xchacha20_poly1305" => Ok(Self::Xchacha20Poly1305),
            "aes256_gcm" => Ok(Self::Aes256Gcm),
            _ => anyhow::bail!("unknown operation cipher suite"),
        }
    }

    pub const fn nonce_len(self) -> usize {
        match self {
            Self::Xchacha20Poly1305 => 24,
            Self::Aes256Gcm => 12,
        }
    }

    const fn tag(self) -> u8 {
        match self {
            Self::Xchacha20Poly1305 => 1,
            Self::Aes256Gcm => 2,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct OperationEnvelopeV1 {
    pub space_id: Uuid,
    pub stream_id: Uuid,
    pub client_op_id: Uuid,
    pub author_device_id: Uuid,
    pub key_epoch: u32,
    pub envelope_kind: EnvelopeKind,
    pub cipher_suite: EnvelopeCipherSuite,
    #[serde(with = "serde_bytes")]
    pub nonce: Vec<u8>,
    #[serde(with = "serde_bytes")]
    pub ciphertext: Vec<u8>,
    #[serde(with = "serde_bytes")]
    pub signature: Vec<u8>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OperationSealContext {
    pub space_id: Uuid,
    pub stream_id: Uuid,
    pub client_op_id: Uuid,
    pub author_device_id: Uuid,
    pub key_epoch: u32,
    pub envelope_kind: EnvelopeKind,
}

impl OperationEnvelopeV1 {
    fn canonical_public_bytes(&self, domain: &[u8]) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(domain.len() + self.nonce.len() + 72);
        bytes.extend_from_slice(domain);
        bytes.extend_from_slice(self.space_id.as_bytes());
        bytes.extend_from_slice(self.stream_id.as_bytes());
        bytes.extend_from_slice(self.client_op_id.as_bytes());
        bytes.extend_from_slice(self.author_device_id.as_bytes());
        bytes.extend_from_slice(&self.key_epoch.to_be_bytes());
        bytes.push(self.envelope_kind.tag());
        bytes.push(self.cipher_suite.tag());
        bytes.extend_from_slice(&(self.nonce.len() as u16).to_be_bytes());
        bytes.extend_from_slice(&self.nonce);
        bytes
    }

    pub fn canonical_aad_bytes(&self) -> Vec<u8> {
        self.canonical_public_bytes(AAD_DOMAIN)
    }

    pub fn canonical_signing_bytes(&self) -> Vec<u8> {
        let mut bytes = self.canonical_public_bytes(SIGNING_DOMAIN);
        bytes.extend_from_slice(&(self.ciphertext.len() as u64).to_be_bytes());
        bytes.extend_from_slice(&self.ciphertext);
        bytes
    }

    pub fn seal_xchacha(
        context: OperationSealContext,
        plaintext: &[u8],
        space_key: &[u8; 32],
        signing_key: &SigningKey,
    ) -> anyhow::Result<Self> {
        if context.space_id.is_nil()
            || context.stream_id.is_nil()
            || context.client_op_id.is_nil()
            || context.author_device_id.is_nil()
        {
            anyhow::bail!("operation envelope ids must be non-nil UUIDs");
        }
        if context.key_epoch == 0 {
            anyhow::bail!("key epoch must be positive");
        }
        let nonce = CryptoEngine::random_nonce_24().to_vec();
        let mut envelope = Self {
            space_id: context.space_id,
            stream_id: context.stream_id,
            client_op_id: context.client_op_id,
            author_device_id: context.author_device_id,
            key_epoch: context.key_epoch,
            envelope_kind: context.envelope_kind,
            cipher_suite: EnvelopeCipherSuite::Xchacha20Poly1305,
            nonce,
            ciphertext: Vec::new(),
            signature: Vec::new(),
        };
        let stream_key = derive_stream_key(
            space_key,
            context.space_id,
            context.stream_id,
            context.key_epoch,
        );
        let encrypted = CryptoEngine::encrypt_payload(
            CipherAlgorithm::XChaCha20Poly1305,
            &stream_key,
            &envelope.nonce,
            plaintext,
            Some(&envelope.canonical_aad_bytes()),
        )?;
        envelope.ciphertext = encrypted.ciphertext;
        envelope.sign(signing_key);
        Ok(envelope)
    }

    pub fn open(&self, space_key: &[u8; 32]) -> anyhow::Result<Vec<u8>> {
        let algorithm = match self.cipher_suite {
            EnvelopeCipherSuite::Xchacha20Poly1305 => CipherAlgorithm::XChaCha20Poly1305,
            EnvelopeCipherSuite::Aes256Gcm => CipherAlgorithm::Aes256Gcm,
        };
        let stream_key =
            derive_stream_key(space_key, self.space_id, self.stream_id, self.key_epoch);
        CryptoEngine::decrypt_payload(
            &crate::EncryptedPayload {
                algorithm,
                nonce: self.nonce.clone(),
                ciphertext: self.ciphertext.clone(),
            },
            &stream_key,
            Some(&self.canonical_aad_bytes()),
        )
    }

    pub fn sign(&mut self, signing_key: &SigningKey) {
        self.signature = signing_key
            .sign(&self.canonical_signing_bytes())
            .to_bytes()
            .to_vec();
    }

    pub fn verify(&self, public_key: &[u8]) -> anyhow::Result<()> {
        let public_key: [u8; 32] = public_key
            .try_into()
            .map_err(|_| anyhow::anyhow!("signing public key must be 32 bytes"))?;
        let signature: [u8; 64] = self
            .signature
            .as_slice()
            .try_into()
            .map_err(|_| anyhow::anyhow!("operation signature must be 64 bytes"))?;
        let verifying_key = VerifyingKey::from_bytes(&public_key)?;
        verifying_key.verify_strict(
            &self.canonical_signing_bytes(),
            &Signature::from_bytes(&signature),
        )?;
        Ok(())
    }
}

fn derive_stream_key(
    space_key: &[u8; 32],
    space_id: Uuid,
    stream_id: Uuid,
    key_epoch: u32,
) -> [u8; 32] {
    let mut info = Vec::with_capacity(STREAM_KEY_DOMAIN.len() + 20);
    info.extend_from_slice(STREAM_KEY_DOMAIN);
    info.extend_from_slice(stream_id.as_bytes());
    info.extend_from_slice(&key_epoch.to_be_bytes());
    CryptoEngine::hkdf_sha256(space_key, Some(space_id.as_bytes()), &info)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unsigned() -> OperationEnvelopeV1 {
        OperationEnvelopeV1 {
            space_id: Uuid::from_u128(1),
            stream_id: Uuid::from_u128(2),
            client_op_id: Uuid::from_u128(3),
            author_device_id: Uuid::from_u128(4),
            key_epoch: 7,
            envelope_kind: EnvelopeKind::Operation,
            cipher_suite: EnvelopeCipherSuite::Xchacha20Poly1305,
            nonce: vec![5; 24],
            ciphertext: vec![6, 7, 8],
            signature: Vec::new(),
        }
    }

    #[test]
    fn signature_covers_every_client_field_and_ciphertext() {
        let signing_key = SigningKey::from_bytes(&[9; 32]);
        let mut envelope = unsigned();
        envelope.sign(&signing_key);
        envelope
            .verify(signing_key.verifying_key().as_bytes())
            .expect("valid signature");

        envelope.ciphertext.push(10);
        assert!(
            envelope
                .verify(signing_key.verifying_key().as_bytes())
                .is_err()
        );
    }

    #[test]
    fn canonical_bytes_are_independent_of_signature() {
        let mut envelope = unsigned();
        let before = envelope.canonical_signing_bytes();
        envelope.signature = vec![42; 64];
        assert_eq!(before, envelope.canonical_signing_bytes());
    }

    #[test]
    fn sealed_envelope_authenticates_public_context() {
        let signing_key = SigningKey::from_bytes(&[9; 32]);
        let space_key = [11; 32];
        let mut envelope = OperationEnvelopeV1::seal_xchacha(
            OperationSealContext {
                space_id: Uuid::from_u128(1),
                stream_id: Uuid::from_u128(2),
                client_op_id: Uuid::from_u128(3),
                author_device_id: Uuid::from_u128(4),
                key_epoch: 1,
                envelope_kind: EnvelopeKind::Operation,
            },
            b"secret operation",
            &space_key,
            &signing_key,
        )
        .expect("seal");
        assert_eq!(
            envelope.open(&space_key).expect("open"),
            b"secret operation"
        );
        envelope.stream_id = Uuid::from_u128(99);
        assert!(envelope.open(&space_key).is_err());
    }
}
