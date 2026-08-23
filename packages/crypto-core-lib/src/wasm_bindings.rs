use super::*;
use getrandom::fill as fill_random;
use opaque_ke::{
    ClientLogin, ClientLoginFinishParameters, ClientRegistration,
    ClientRegistrationFinishParameters, CredentialResponse, RegistrationResponse, Ristretto255,
    argon2::Argon2, ciphersuite::CipherSuite, key_exchange::tripledh::TripleDh,
};
use qrcode::{QrCode, render::svg};
use rand_core::OsRng;
use serde::{Deserialize, Serialize};
use sha2_opaque::Sha512;
use std::{cell::RefCell, collections::HashMap};
use wasm_bindgen::prelude::*;

use crate::{
    operation_envelope::{EnvelopeKind, OperationEnvelopeV1},
    pim::{PimBranchNodeV1, PimOperationV1, assign_pim_branches, materialize_projection},
};

pub struct WebOpaqueSuite;

impl CipherSuite for WebOpaqueSuite {
    type OprfCs = Ristretto255;
    type KeyExchange = TripleDh<Ristretto255, Sha512>;
    type Ksf = Argon2<'static>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct OpaqueClientStartOutput {
    flow_id: String,
    #[serde(with = "serde_bytes")]
    opaque_start_request: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct OpaqueClientFinishOutput {
    #[serde(with = "serde_bytes")]
    opaque_finish_request: Vec<u8>,
    #[serde(with = "serde_bytes")]
    export_key: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct WebDeviceIdentity {
    #[serde(with = "serde_bytes")]
    signing_private_key: Vec<u8>,
    #[serde(with = "serde_bytes")]
    signing_public_key: Vec<u8>,
    #[serde(with = "serde_bytes")]
    hpke_private_key: Vec<u8>,
    #[serde(with = "serde_bytes")]
    hpke_public_key: Vec<u8>,
}

thread_local! {
    static WEB_SIGNUP_STATES: RefCell<HashMap<String, ClientRegistration<WebOpaqueSuite>>> =
        RefCell::new(HashMap::new());
    static WEB_SIGNIN_STATES: RefCell<HashMap<String, ClientLogin<WebOpaqueSuite>>> =
        RefCell::new(HashMap::new());
}

fn js_error(message: impl Into<String>) -> JsValue {
    JsValue::from_str(&message.into())
}

fn opaque_rng() -> OsRng {
    OsRng
}

fn random_flow_id() -> Result<String, JsValue> {
    let mut bytes = [0u8; 16];
    fill_random(&mut bytes)
        .map_err(|error| js_error(format!("flow id generation failed: {error}")))?;
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}

#[wasm_bindgen]
pub fn opaque_signup_start(password: Vec<u8>) -> Result<JsValue, JsValue> {
    if password.is_empty() {
        return Err(js_error("password is required"));
    }

    let mut rng = opaque_rng();
    let start = ClientRegistration::<WebOpaqueSuite>::start(&mut rng, password.as_slice())
        .map_err(|error| js_error(format!("opaque signup start failed: {error:?}")))?;
    let flow_id = random_flow_id()?;

    WEB_SIGNUP_STATES.with(|states| {
        states.borrow_mut().insert(flow_id.clone(), start.state);
    });

    serde_wasm_bindgen::to_value(&OpaqueClientStartOutput {
        flow_id,
        opaque_start_request: start.message.serialize().to_vec(),
    })
    .map_err(|error| js_error(format!("serialize signup start output failed: {error}")))
}

#[wasm_bindgen]
pub fn opaque_signup_finish(
    flow_id: String,
    password: Vec<u8>,
    opaque_server_message: Vec<u8>,
) -> Result<JsValue, JsValue> {
    if password.is_empty() {
        return Err(js_error("password is required"));
    }
    if flow_id.trim().is_empty() {
        return Err(js_error("flow id is required"));
    }

    let state = WEB_SIGNUP_STATES.with(|states| states.borrow_mut().remove(&flow_id));
    let Some(state) = state else {
        return Err(js_error("opaque signup flow state not found or expired"));
    };

    let registration_response = RegistrationResponse::<WebOpaqueSuite>::deserialize(
        &opaque_server_message,
    )
    .map_err(|error| {
        js_error(format!(
            "opaque signup server message decode failed: {error:?}"
        ))
    })?;

    let mut rng = opaque_rng();
    let finish = state
        .finish(
            &mut rng,
            password.as_slice(),
            registration_response,
            ClientRegistrationFinishParameters::default(),
        )
        .map_err(|error| js_error(format!("opaque signup finish failed: {error:?}")))?;

    serde_wasm_bindgen::to_value(&OpaqueClientFinishOutput {
        opaque_finish_request: finish.message.serialize().to_vec(),
        export_key: finish.export_key.as_slice().to_vec(),
    })
    .map_err(|error| js_error(format!("serialize signup finish output failed: {error}")))
}

#[wasm_bindgen]
pub fn opaque_signin_start(password: Vec<u8>) -> Result<JsValue, JsValue> {
    if password.is_empty() {
        return Err(js_error("password is required"));
    }

    let mut rng = opaque_rng();
    let start = ClientLogin::<WebOpaqueSuite>::start(&mut rng, password.as_slice())
        .map_err(|error| js_error(format!("opaque signin start failed: {error:?}")))?;
    let flow_id = random_flow_id()?;

    WEB_SIGNIN_STATES.with(|states| {
        states.borrow_mut().insert(flow_id.clone(), start.state);
    });

    serde_wasm_bindgen::to_value(&OpaqueClientStartOutput {
        flow_id,
        opaque_start_request: start.message.serialize().to_vec(),
    })
    .map_err(|error| js_error(format!("serialize signin start output failed: {error}")))
}

#[wasm_bindgen]
pub fn opaque_signin_finish(
    flow_id: String,
    password: Vec<u8>,
    opaque_server_message: Vec<u8>,
) -> Result<JsValue, JsValue> {
    if password.is_empty() {
        return Err(js_error("password is required"));
    }
    if flow_id.trim().is_empty() {
        return Err(js_error("flow id is required"));
    }

    let state = WEB_SIGNIN_STATES.with(|states| states.borrow_mut().remove(&flow_id));
    let Some(state) = state else {
        return Err(js_error("opaque signin flow state not found or expired"));
    };

    let credential_response = CredentialResponse::<WebOpaqueSuite>::deserialize(
        &opaque_server_message,
    )
    .map_err(|error| {
        js_error(format!(
            "opaque signin server message decode failed: {error:?}"
        ))
    })?;

    let mut rng = opaque_rng();
    let finish = state
        .finish(
            &mut rng,
            password.as_slice(),
            credential_response,
            ClientLoginFinishParameters::default(),
        )
        .map_err(|error| js_error(format!("opaque signin finish failed: {error:?}")))?;

    serde_wasm_bindgen::to_value(&OpaqueClientFinishOutput {
        opaque_finish_request: finish.message.serialize().to_vec(),
        export_key: finish.export_key.as_slice().to_vec(),
    })
    .map_err(|error| js_error(format!("serialize signin finish output failed: {error}")))
}

#[wasm_bindgen]
pub fn generate_web_device_identity() -> Result<JsValue, JsValue> {
    let hpke = CryptoEngine::generate_x25519_keypair();
    let mut signing_private_key = [0_u8; 32];
    fill_random(&mut signing_private_key)
        .map_err(|error| js_error(format!("device signing key generation failed: {error}")))?;
    let signing = ed25519_dalek::SigningKey::from_bytes(&signing_private_key);
    serde_wasm_bindgen::to_value(&WebDeviceIdentity {
        signing_private_key: signing_private_key.to_vec(),
        signing_public_key: signing.verifying_key().as_bytes().to_vec(),
        hpke_private_key: hpke.private_key.to_vec(),
        hpke_public_key: hpke.public_key.to_vec(),
    })
    .map_err(|error| js_error(format!("serialize web device identity failed: {error}")))
}

#[wasm_bindgen]
pub fn encrypt_vault_bytes(master_key: Vec<u8>, plaintext: Vec<u8>) -> Result<Vec<u8>, JsValue> {
    let master_key: [u8; 32] = master_key
        .try_into()
        .map_err(|_| js_error("master key must be 32 bytes"))?;
    crate::secret_vault::encrypt(&master_key, &plaintext)
        .map_err(|error| js_error(format!("secret vault encryption failed: {error}")))
}

#[wasm_bindgen]
pub fn decrypt_vault_bytes(master_key: Vec<u8>, encrypted: Vec<u8>) -> Result<Vec<u8>, JsValue> {
    let master_key: [u8; 32] = master_key
        .try_into()
        .map_err(|_| js_error("master key must be 32 bytes"))?;
    crate::secret_vault::decrypt(&master_key, &encrypted)
        .map_err(|error| js_error(format!("secret vault decryption failed: {error}")))
}

#[wasm_bindgen]
pub fn wrap_account_master_key(
    export_key: Vec<u8>,
    master_key: Vec<u8>,
) -> Result<Vec<u8>, JsValue> {
    let master_key: [u8; 32] = master_key
        .try_into()
        .map_err(|_| js_error("master key must be 32 bytes"))?;
    crate::account_keys::wrap(&export_key, &master_key)
        .map_err(|error| js_error(format!("account master key wrap failed: {error}")))
}

#[wasm_bindgen]
pub fn unwrap_account_master_key(
    export_key: Vec<u8>,
    encrypted: Vec<u8>,
) -> Result<Vec<u8>, JsValue> {
    crate::account_keys::unwrap(&export_key, &encrypted)
        .map(|master_key| master_key.to_vec())
        .map_err(|error| js_error(format!("account master key unwrap failed: {error}")))
}

/// Encodes the 256-bit account master key as a checksummed 24-word BIP-39 kit.
#[wasm_bindgen]
pub fn master_key_to_recovery_phrase(master_key: Vec<u8>) -> Result<String, JsValue> {
    let master_key: [u8; 32] = master_key
        .try_into()
        .map_err(|_| js_error("master key must be 32 bytes"))?;
    crate::recovery::encode_master_key(&master_key)
        .map_err(|error| js_error(format!("recovery kit generation failed: {error}")))
}

/// Validates a 24-word BIP-39 kit and restores its exact account master key.
#[wasm_bindgen]
pub fn recovery_phrase_to_master_key(phrase: String) -> Result<Vec<u8>, JsValue> {
    crate::recovery::decode_master_key(&phrase)
        .map(|master_key| master_key.to_vec())
        .map_err(|_| js_error("recovery kit is invalid"))
}

#[wasm_bindgen]
#[allow(clippy::too_many_arguments)]
pub fn seal_operation_envelope(
    space_id: String,
    stream_id: String,
    client_op_id: String,
    author_device_id: String,
    key_epoch: u32,
    envelope_kind: String,
    plaintext: Vec<u8>,
    space_key: Vec<u8>,
    signing_private_key: Vec<u8>,
) -> Result<JsValue, JsValue> {
    let parse_uuid = |field: &str, value: &str| {
        uuid::Uuid::parse_str(value).map_err(|error| js_error(format!("invalid {field}: {error}")))
    };
    let space_key: [u8; 32] = space_key
        .try_into()
        .map_err(|_| js_error("space key must be 32 bytes"))?;
    let signing_private_key: [u8; 32] = signing_private_key
        .try_into()
        .map_err(|_| js_error("signing private key must be 32 bytes"))?;
    let kind = match envelope_kind.as_str() {
        "operation" => EnvelopeKind::Operation,
        "snapshot" => EnvelopeKind::Snapshot,
        "control" => EnvelopeKind::Control,
        _ => return Err(js_error("invalid envelope kind")),
    };
    let envelope = OperationEnvelopeV1::seal_xchacha(
        crate::operation_envelope::OperationSealContext {
            space_id: parse_uuid("space_id", &space_id)?,
            stream_id: parse_uuid("stream_id", &stream_id)?,
            client_op_id: parse_uuid("client_op_id", &client_op_id)?,
            author_device_id: parse_uuid("author_device_id", &author_device_id)?,
            key_epoch,
            envelope_kind: kind,
        },
        &plaintext,
        &space_key,
        &ed25519_dalek::SigningKey::from_bytes(&signing_private_key),
    )
    .map_err(|error| js_error(format!("seal operation envelope failed: {error}")))?;
    serde_wasm_bindgen::to_value(&envelope)
        .map_err(|error| js_error(format!("serialize operation envelope failed: {error}")))
}

#[wasm_bindgen]
pub fn open_operation_envelope(envelope: JsValue, space_key: Vec<u8>) -> Result<Vec<u8>, JsValue> {
    let envelope: OperationEnvelopeV1 = serde_wasm_bindgen::from_value(envelope)
        .map_err(|error| js_error(format!("decode operation envelope failed: {error}")))?;
    let space_key: [u8; 32] = space_key
        .try_into()
        .map_err(|_| js_error("space key must be 32 bytes"))?;
    envelope
        .open(&space_key)
        .map_err(|error| js_error(format!("open operation envelope failed: {error}")))
}

#[wasm_bindgen]
pub fn verify_operation_envelope(
    envelope: JsValue,
    signing_public_key: Vec<u8>,
) -> Result<(), JsValue> {
    let envelope: OperationEnvelopeV1 = serde_wasm_bindgen::from_value(envelope)
        .map_err(|error| js_error(format!("decode operation envelope failed: {error}")))?;
    envelope
        .verify(&signing_public_key)
        .map_err(|_| js_error("operation envelope signature is invalid"))
}

#[wasm_bindgen]
pub fn materialize_pim_operation(
    operation: JsValue,
    existing_projection: Option<String>,
) -> Result<String, JsValue> {
    let operation: PimOperationV1 = serde_wasm_bindgen::from_value(operation)
        .map_err(|error| js_error(format!("decode PIM operation failed: {error}")))?;
    operation
        .validate()
        .map_err(|error| js_error(format!("validate PIM operation failed: {error}")))?;
    let PimOperationV1::Upsert(upsert) = operation else {
        return Err(js_error("only PIM upserts have a materialized projection"));
    };
    materialize_projection(&upsert, existing_projection.as_deref())
        .map_err(|error| js_error(format!("materialize PIM projection failed: {error}")))
}

#[wasm_bindgen]
pub fn assign_pim_branch_graph(
    default_projection_resource_id: String,
    nodes: JsValue,
) -> Result<JsValue, JsValue> {
    let nodes: Vec<PimBranchNodeV1> = serde_wasm_bindgen::from_value(nodes)
        .map_err(|error| js_error(format!("decode PIM branch graph failed: {error}")))?;
    let assignments = assign_pim_branches(&default_projection_resource_id, &nodes)
        .map_err(|error| js_error(format!("assign PIM branches failed: {error}")))?;
    serde_wasm_bindgen::to_value(&assignments)
        .map_err(|error| js_error(format!("serialize PIM branch graph failed: {error}")))
}

#[wasm_bindgen]
pub fn generate_x25519_keypair() -> Result<JsValue, JsValue> {
    let kp = CryptoEngine::generate_x25519_keypair();
    serde_wasm_bindgen::to_value(&kp)
        .map_err(|error| js_error(format!("serialize keypair failed: {error}")))
}

#[wasm_bindgen]
pub fn derive_account_recovery_keypair(master_key: Vec<u8>) -> Result<JsValue, JsValue> {
    let master_key: [u8; 32] = master_key
        .try_into()
        .map_err(|_| js_error("account master key must be 32 bytes"))?;
    let keypair = CryptoEngine::derive_account_recovery_keypair(&master_key);
    serde_wasm_bindgen::to_value(&keypair)
        .map_err(|error| js_error(format!("serialize recovery keypair failed: {error}")))
}

#[wasm_bindgen]
pub fn encrypt_payload(
    algorithm: JsValue,
    key: Vec<u8>,
    nonce: Vec<u8>,
    plaintext: Vec<u8>,
    aad: Vec<u8>,
) -> Result<JsValue, JsValue> {
    let alg: CipherAlgorithm = serde_wasm_bindgen::from_value(algorithm)
        .map_err(|error| js_error(format!("decode cipher algorithm failed: {error}")))?;
    let key: [u8; 32] = key
        .try_into()
        .map_err(|_| js_error("encryption key must be 32 bytes"))?;
    let aad_opt = if aad.is_empty() {
        None
    } else {
        Some(aad.as_slice())
    };
    let encrypted = CryptoEngine::encrypt_payload(alg, &key, &nonce, &plaintext, aad_opt)
        .map_err(|error| js_error(format!("encrypt payload failed: {error}")))?;
    serde_wasm_bindgen::to_value(&encrypted)
        .map_err(|error| js_error(format!("serialize encrypted payload failed: {error}")))
}

#[wasm_bindgen]
pub fn decrypt_payload(encrypted: JsValue, key: Vec<u8>, aad: Vec<u8>) -> Result<Vec<u8>, JsValue> {
    let encrypted: EncryptedPayload = serde_wasm_bindgen::from_value(encrypted)
        .map_err(|error| js_error(format!("decode encrypted payload failed: {error}")))?;
    let key: [u8; 32] = key
        .try_into()
        .map_err(|_| js_error("decryption key must be 32 bytes"))?;
    let aad_opt = if aad.is_empty() {
        None
    } else {
        Some(aad.as_slice())
    };
    CryptoEngine::decrypt_payload(&encrypted, &key, aad_opt)
        .map_err(|error| js_error(format!("decrypt payload failed: {error}")))
}

#[wasm_bindgen]
pub fn encrypt_group_key_for_peer(
    cmk: Vec<u8>,
    peer_public_key: Vec<u8>,
) -> Result<JsValue, JsValue> {
    let cmk: [u8; 32] = cmk
        .try_into()
        .map_err(|_| js_error("space key must be 32 bytes"))?;
    let peer_public_key: [u8; 32] = peer_public_key
        .try_into()
        .map_err(|_| js_error("peer public key must be 32 bytes"))?;
    let encrypted = CryptoEngine::encrypt_group_key_for_peer(&cmk, &peer_public_key)
        .map_err(|error| js_error(format!("encrypt group key failed: {error}")))?;
    serde_wasm_bindgen::to_value(&encrypted)
        .map_err(|error| js_error(format!("serialize encrypted group key failed: {error}")))
}

#[wasm_bindgen]
pub fn decrypt_group_key_from_peer(
    encrypted: JsValue,
    recipient_private_key: Vec<u8>,
) -> Result<Vec<u8>, JsValue> {
    let encrypted: EncryptedGroupKey = serde_wasm_bindgen::from_value(encrypted)
        .map_err(|error| js_error(format!("decode encrypted group key failed: {error}")))?;
    let recipient_private_key: [u8; 32] = recipient_private_key
        .try_into()
        .map_err(|_| js_error("recipient private key must be 32 bytes"))?;
    CryptoEngine::decrypt_group_key_from_peer(&encrypted, &recipient_private_key)
        .map(|key| key.to_vec())
        .map_err(|error| js_error(format!("decrypt group key failed: {error}")))
}

#[wasm_bindgen]
pub fn encrypt_account_master_key_for_device(
    master_key: Vec<u8>,
    peer_public_key: Vec<u8>,
    flow_id: String,
) -> Result<Vec<u8>, JsValue> {
    let master_key: [u8; 32] = master_key
        .try_into()
        .map_err(|_| js_error("account master key must be 32 bytes"))?;
    let peer_public_key: [u8; 32] = peer_public_key
        .try_into()
        .map_err(|_| js_error("device HPKE public key must be 32 bytes"))?;
    let flow_id = uuid::Uuid::parse_str(&flow_id)
        .map_err(|_| js_error("device authorization flow id is invalid"))?;
    let encrypted = CryptoEngine::encrypt_device_bootstrap(&master_key, &peer_public_key, flow_id)
        .map_err(|error| js_error(format!("device bootstrap encryption failed: {error}")))?;
    rmp_serde::to_vec_named(&encrypted).map_err(|error| {
        js_error(format!(
            "serialize device bootstrap package failed: {error}"
        ))
    })
}

#[wasm_bindgen]
pub fn generate_qr_svg(payload: String) -> Result<String, JsValue> {
    if payload.trim().is_empty() {
        return Err(js_error("qr payload is required"));
    }
    let code = QrCode::new(payload.as_bytes())
        .map_err(|error| js_error(format!("failed to build qr: {error}")))?;
    Ok(code
        .render::<svg::Color>()
        .min_dimensions(240, 240)
        .quiet_zone(true)
        .build())
}
