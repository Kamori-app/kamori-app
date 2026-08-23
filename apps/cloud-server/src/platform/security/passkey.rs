//! Passkey authentication helpers backed by Valkey state storage.

use crate::platform::config::Config;
use crate::platform::state_store::{StateStore, StateStoreError};
use anyhow::{Result, anyhow};
use base64::Engine;
use base64::engine::general_purpose::{URL_SAFE, URL_SAFE_NO_PAD};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;
use std::time::Duration;
use url::Url;
use uuid::Uuid;
use webauthn_rs::prelude::{
    AttestationCaList, AuthenticationResult, AuthenticatorAttachment, DiscoverableAuthentication,
    DiscoverableKey, Passkey, PasskeyRegistration, PublicKeyCredential,
    RegisterPublicKeyCredential, SecurityKey, SecurityKeyAuthentication, SecurityKeyRegistration,
    Webauthn, WebauthnBuilder,
};
use webauthn_rs_device_catalog::Data;

/// In-memory passkey service state using Valkey for challenge storage.
#[derive(Clone)]
pub struct PasskeyService {
    webauthn: Webauthn,
    state_store: Arc<dyn StateStore>,
    challenge_ttl: Duration,
    security_key_attestation_ca_list: Option<AttestationCaList>,
}

/// Envelope for passkey challenge and options.
#[derive(Debug, Clone)]
pub struct PasskeyChallenge {
    /// Raw challenge bytes.
    pub challenge: Vec<u8>,
    /// Serialized PublicKeyCredentialRequestOptions.
    pub public_key_credential_request_options: Vec<u8>,
}

/// Envelope for passkey registration challenge and options.
#[derive(Debug, Clone)]
pub struct PasskeyRegistrationChallenge {
    /// Raw challenge bytes.
    pub challenge: Vec<u8>,
    /// Serialized PublicKeyCredentialCreationOptions.
    pub public_key_credential_creation_options: Vec<u8>,
}

#[derive(Debug, Serialize, Deserialize)]
struct UserPasskeyRegistrationState {
    user_id: Uuid,
    registration: PasskeyRegistration,
}

impl PasskeyService {
    /// Builds a passkey service instance using Valkey state storage.
    pub fn new(config: &Config, state_store: Arc<dyn StateStore>) -> Result<Self> {
        let webauthn = build_webauthn(
            &config.webauthn_rp_id,
            &config.webauthn_rp_origin,
            &config.webauthn_rp_name,
        )?;
        Ok(Self {
            webauthn,
            state_store,
            challenge_ttl: Duration::from_secs(config.valkey_ttl_seconds),
            security_key_attestation_ca_list: None,
        })
    }

    /// Builds the separate operator WebAuthn verifier with an exact admin origin.
    pub fn new_admin(config: &Config, state_store: Arc<dyn StateStore>) -> Result<Self> {
        let webauthn = build_webauthn(
            &config.webauthn_rp_id,
            &config.admin_webauthn_rp_origin,
            &config.admin_webauthn_rp_name,
        )?;
        let device_catalog = Data::strict();
        let mut attestation_ca_list = AttestationCaList::default();
        for device in device_catalog.iter() {
            for authority in &device.aaguid.ca {
                let pem = authority.ca.to_pem().map_err(|error| {
                    anyhow!("encode strict security-key attestation CA: {error:?}")
                })?;
                let next = AttestationCaList::try_from(pem.as_slice()).map_err(|error| {
                    anyhow!("parse strict security-key attestation CA: {error:?}")
                })?;
                attestation_ca_list.union(&next);
            }
        }
        if attestation_ca_list.is_empty() {
            anyhow::bail!("strict security-key attestation catalog is empty");
        }
        let security_key_attestation_ca_list = Some(attestation_ca_list);
        Ok(Self {
            webauthn,
            state_store,
            challenge_ttl: Duration::from_secs(config.valkey_ttl_seconds),
            security_key_attestation_ca_list,
        })
    }

    /// Returns the underlying Webauthn instance.
    pub fn webauthn(&self) -> &Webauthn {
        &self.webauthn
    }

    /// Starts passkey registration and stores state under `flow_id`.
    pub async fn start_registration(
        &self,
        flow_id: Uuid,
        user_id: Uuid,
        username: &str,
        display_name: &str,
    ) -> Result<PasskeyRegistrationChallenge> {
        let (creation_options, state) = self
            .webauthn
            .start_passkey_registration(user_id, username, display_name, None)
            .map_err(|e| anyhow!("passkey registration start failed: {e:?}"))?;

        let state_bytes = serde_json::to_vec(&UserPasskeyRegistrationState {
            user_id,
            registration: state,
        })
        .map_err(|e| anyhow!("serialize passkey state: {e}"))?;
        let key = registration_flow_key(flow_id);
        self.state_store
            .put(&key, &state_bytes, self.challenge_ttl)
            .await
            .map_err(map_store_error)?;

        let creation_options_bytes = serde_json::to_vec(&creation_options)
            .map_err(|e| anyhow!("serialize passkey creation options: {e}"))?;
        let challenge = extract_challenge_from_json(&creation_options_bytes)?;

        Ok(PasskeyRegistrationChallenge {
            challenge,
            public_key_credential_creation_options: creation_options_bytes,
        })
    }

    /// Finishes passkey registration by `flow_id`.
    pub async fn finish_registration(
        &self,
        flow_id: Uuid,
        expected_user_id: Uuid,
        credential: RegisterPublicKeyCredential,
    ) -> Result<Passkey> {
        let key = registration_flow_key(flow_id);
        let state_bytes = self
            .state_store
            .take(&key)
            .await
            .map_err(map_store_error)?
            .ok_or_else(|| anyhow!("missing passkey registration state"))?;

        let state: UserPasskeyRegistrationState = serde_json::from_slice(&state_bytes)
            .map_err(|e| anyhow!("deserialize passkey state: {e}"))?;
        if state.user_id != expected_user_id {
            anyhow::bail!("passkey registration account mismatch");
        }

        self.webauthn
            .finish_passkey_registration(&credential, &state.registration)
            .map_err(|e| anyhow!("passkey registration finish failed: {e:?}"))
    }

    /// Starts username-less discoverable passkey authentication and stores state under `flow_id`.
    pub async fn start_discoverable_authentication(
        &self,
        flow_id: Uuid,
    ) -> Result<PasskeyChallenge> {
        let (request_options, state) = self
            .webauthn
            .start_discoverable_authentication()
            .map_err(|e| anyhow!("passkey discoverable login start failed: {e:?}"))?;

        let state_bytes =
            serde_json::to_vec(&state).map_err(|e| anyhow!("serialize passkey state: {e}"))?;
        let key = discoverable_authentication_key(flow_id);
        self.state_store
            .put(&key, &state_bytes, self.challenge_ttl)
            .await
            .map_err(map_store_error)?;

        let request_options_bytes = serde_json::to_vec(&request_options)
            .map_err(|e| anyhow!("serialize passkey request options: {e}"))?;
        let challenge = extract_challenge_from_json(&request_options_bytes)?;

        Ok(PasskeyChallenge {
            challenge,
            public_key_credential_request_options: request_options_bytes,
        })
    }

    /// Finishes username-less discoverable passkey authentication.
    pub async fn finish_discoverable_authentication(
        &self,
        flow_id: Uuid,
        credential: PublicKeyCredential,
        discoverable_keys: &[DiscoverableKey],
    ) -> Result<AuthenticationResult> {
        let key = discoverable_authentication_key(flow_id);
        let state_bytes = self
            .state_store
            .take(&key)
            .await
            .map_err(map_store_error)?
            .ok_or_else(|| anyhow!("missing passkey discoverable authentication state"))?;

        let state: DiscoverableAuthentication = serde_json::from_slice(&state_bytes)
            .map_err(|e| anyhow!("deserialize passkey state: {e}"))?;

        self.webauthn
            .finish_discoverable_authentication(&credential, state, discoverable_keys)
            .map_err(|e| anyhow!("passkey discoverable login finish failed: {e:?}"))
    }

    /// Starts registration restricted to a roaming security-key UX.
    pub async fn start_security_key_registration(
        &self,
        flow_id: Uuid,
        user_id: Uuid,
        username: &str,
    ) -> Result<PasskeyRegistrationChallenge> {
        let (creation_options, state) = self
            .webauthn
            .start_securitykey_registration(
                user_id,
                username,
                username,
                None,
                self.security_key_attestation_ca_list.clone(),
                Some(AuthenticatorAttachment::CrossPlatform),
            )
            .map_err(|error| anyhow!("security-key registration start failed: {error:?}"))?;
        self.state_store
            .put(
                &security_key_registration_flow_key(flow_id),
                &serde_json::to_vec(&state)?,
                self.challenge_ttl,
            )
            .await
            .map_err(map_store_error)?;
        let options = serde_json::to_vec(&creation_options)?;
        Ok(PasskeyRegistrationChallenge {
            challenge: extract_challenge_from_json(&options)?,
            public_key_credential_creation_options: options,
        })
    }

    /// Completes a roaming security-key registration flow.
    pub async fn finish_security_key_registration(
        &self,
        flow_id: Uuid,
        credential: RegisterPublicKeyCredential,
    ) -> Result<SecurityKey> {
        let key = security_key_registration_flow_key(flow_id);
        let state = self
            .state_store
            .take(&key)
            .await
            .map_err(map_store_error)?
            .ok_or_else(|| anyhow!("missing security-key registration state"))?;
        let state: SecurityKeyRegistration = serde_json::from_slice(&state)?;
        self.webauthn
            .finish_securitykey_registration(&credential, &state)
            .map_err(|error| anyhow!("security-key registration finish failed: {error:?}"))
    }

    /// Starts authentication against an explicit set of operator security keys.
    pub async fn start_security_key_authentication(
        &self,
        flow_id: Uuid,
        keys: &[SecurityKey],
    ) -> Result<PasskeyChallenge> {
        let (request_options, state) = self
            .webauthn
            .start_securitykey_authentication(keys)
            .map_err(|error| anyhow!("security-key authentication start failed: {error:?}"))?;
        self.state_store
            .put(
                &security_key_authentication_flow_key(flow_id),
                &serde_json::to_vec(&state)?,
                self.challenge_ttl,
            )
            .await
            .map_err(map_store_error)?;
        let options = serde_json::to_vec(&request_options)?;
        Ok(PasskeyChallenge {
            challenge: extract_challenge_from_json(&options)?,
            public_key_credential_request_options: options,
        })
    }

    /// Completes an operator security-key authentication flow.
    pub async fn finish_security_key_authentication(
        &self,
        flow_id: Uuid,
        credential: PublicKeyCredential,
    ) -> Result<AuthenticationResult> {
        let key = security_key_authentication_flow_key(flow_id);
        let state = self
            .state_store
            .take(&key)
            .await
            .map_err(map_store_error)?
            .ok_or_else(|| anyhow!("missing security-key authentication state"))?;
        let state: SecurityKeyAuthentication = serde_json::from_slice(&state)?;
        self.webauthn
            .finish_securitykey_authentication(&credential, &state)
            .map_err(|error| anyhow!("security-key authentication failed: {error:?}"))
    }
}

/// Builds a Webauthn instance from configuration.
pub fn build_webauthn(rp_id: &str, rp_origin: &str, rp_name: &str) -> Result<Webauthn> {
    let url =
        Url::parse(rp_origin).map_err(|e| anyhow!("failed to parse webauthn rp origin :{e:?}"))?;
    WebauthnBuilder::new(rp_id, &url)?
        .rp_name(rp_name)
        .build()
        .map_err(|e| anyhow!("failed to build webauthn: {e:?}"))
}

/// Serializes a passkey into bytes.
pub fn encode_passkey(passkey: &Passkey) -> Result<Vec<u8>> {
    serde_json::to_vec(passkey).map_err(|e| anyhow!("serialize passkey: {e}"))
}

/// Deserializes a passkey from bytes.
pub fn decode_passkey(bytes: &[u8]) -> Result<Passkey> {
    serde_json::from_slice(bytes).map_err(|e| anyhow!("deserialize passkey: {e}"))
}

/// Serializes an operator security key into bytes.
pub fn encode_security_key(key: &SecurityKey) -> Result<Vec<u8>> {
    serde_json::to_vec(key).map_err(|error| anyhow!("serialize security key: {error}"))
}

/// Deserializes an operator security key from bytes.
pub fn decode_security_key(bytes: &[u8]) -> Result<SecurityKey> {
    serde_json::from_slice(bytes).map_err(|error| anyhow!("deserialize security key: {error}"))
}

/// Extracts challenge bytes from serialized options.
fn extract_challenge_from_json(bytes: &[u8]) -> Result<Vec<u8>> {
    let value: Value =
        serde_json::from_slice(bytes).map_err(|e| anyhow!("deserialize passkey options: {e}"))?;

    let challenge_value = value
        .get("challenge")
        .or_else(|| value.get("publicKey").and_then(|v| v.get("challenge")))
        .or_else(|| value.get("public_key").and_then(|v| v.get("challenge")))
        .ok_or_else(|| anyhow!("missing challenge in passkey options"))?;

    match challenge_value {
        Value::String(s) => {
            let decoded = URL_SAFE_NO_PAD
                .decode(s.as_bytes())
                .or_else(|_| URL_SAFE.decode(s.as_bytes()))
                .map_err(|e| anyhow!("decode passkey challenge: {e}"))?;
            Ok(decoded)
        }
        Value::Array(items) => {
            let mut out = Vec::with_capacity(items.len());
            for item in items {
                let v = item
                    .as_u64()
                    .ok_or_else(|| anyhow!("invalid challenge byte"))?;
                if v > 255 {
                    return Err(anyhow!("invalid challenge byte"));
                }
                out.push(v as u8);
            }
            Ok(out)
        }
        Value::Object(map) => {
            if let Some(Value::Array(items)) = map.get("data") {
                let mut out = Vec::with_capacity(items.len());
                for item in items {
                    let v = item
                        .as_u64()
                        .ok_or_else(|| anyhow!("invalid challenge byte"))?;
                    if v > 255 {
                        return Err(anyhow!("invalid challenge byte"));
                    }
                    out.push(v as u8);
                }
                Ok(out)
            } else {
                Err(anyhow!("unsupported challenge format"))
            }
        }
        _ => Err(anyhow!("unsupported challenge format")),
    }
}

/// Key used for discoverable authentication state storage.
fn discoverable_authentication_key(flow_id: Uuid) -> String {
    format!("passkey:auth:flow:{flow_id}")
}

/// Key used for registration state storage.
fn registration_flow_key(flow_id: Uuid) -> String {
    format!("passkey:reg:flow:{flow_id}")
}

fn security_key_registration_flow_key(flow_id: Uuid) -> String {
    format!("admin:security-key:reg:{flow_id}")
}

fn security_key_authentication_flow_key(flow_id: Uuid) -> String {
    format!("admin:security-key:auth:{flow_id}")
}

/// Maps store errors into anyhow.
fn map_store_error(err: StateStoreError) -> anyhow::Error {
    anyhow!("state store error: {err}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_challenge_from_json_accepts_base64url() {
        let challenge = URL_SAFE_NO_PAD.encode(b"test");
        let json = format!(r#"{{"challenge":"{}"}}"#, challenge);
        let bytes = extract_challenge_from_json(json.as_bytes()).expect("extract");
        assert_eq!(bytes, b"test");
    }

    #[test]
    fn extract_challenge_from_json_accepts_array() {
        let json = r#"{"challenge":[1,2,3]}"#;
        let bytes = extract_challenge_from_json(json.as_bytes()).expect("extract");
        assert_eq!(bytes, vec![1, 2, 3]);
    }
}
