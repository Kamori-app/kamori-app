import initOpaqueWasm, {
  decrypt_vault_bytes,
  encrypt_vault_bytes,
  generate_web_device_identity,
  generate_qr_svg,
  encrypt_group_key_for_peer,
  decrypt_group_key_from_peer,
  opaque_signin_finish,
  opaque_signin_start,
  opaque_signup_finish,
  opaque_signup_start,
  open_operation_envelope,
  master_key_to_recovery_phrase,
  recovery_phrase_to_master_key,
  seal_operation_envelope,
  unwrap_account_master_key,
  verify_operation_envelope,
  wrap_account_master_key,
} from "$lib/wasm/crypto-core-lib/crypto_core_lib.js";

/**
 * Browser-side OPAQUE helpers backed by generated wasm-bindgen exports.
 */
const textEncoder = new TextEncoder();

let initPromise: Promise<void> | null = null;

interface OpaqueStartPayload {
  flow_id: string;
  opaque_start_request: Uint8Array;
}

interface OpaqueFinishPayload {
  opaque_finish_request: Uint8Array;
  export_key: Uint8Array;
}

export interface WebDeviceIdentity {
  signing_private_key: Uint8Array;
  signing_public_key: Uint8Array;
  hpke_private_key: Uint8Array;
  hpke_public_key: Uint8Array;
}

export interface OperationEnvelopeV1 {
  space_id: string;
  stream_id: string;
  client_op_id: string;
  author_device_id: string;
  key_epoch: number;
  envelope_kind: "operation" | "snapshot" | "control";
  cipher_suite: "xchacha20_poly1305" | "aes256_gcm";
  nonce: Uint8Array;
  ciphertext: Uint8Array;
  signature: Uint8Array;
}

/**
 * Initializes OPAQUE wasm runtime exactly once per page lifetime.
 */
const ensureOpaqueRuntime = async (): Promise<void> => {
  if (!initPromise) {
    initPromise = initOpaqueWasm().then(() => undefined);
  }
  await initPromise;
};

/**
 * Starts OPAQUE signup client flow.
 */
export const opaqueSignupStart = async (
  password: string,
): Promise<OpaqueStartPayload> => {
  await ensureOpaqueRuntime();
  return opaque_signup_start(
    textEncoder.encode(password),
  ) as OpaqueStartPayload;
};

/**
 * Finishes OPAQUE signup client flow.
 */
export const opaqueSignupFinish = async (
  flowId: string,
  password: string,
  opaqueServerMessage: Uint8Array,
): Promise<OpaqueFinishPayload> => {
  await ensureOpaqueRuntime();
  return opaque_signup_finish(
    flowId,
    textEncoder.encode(password),
    opaqueServerMessage,
  ) as OpaqueFinishPayload;
};

/**
 * Starts OPAQUE signin client flow.
 */
export const opaqueSigninStart = async (
  password: string,
): Promise<OpaqueStartPayload> => {
  await ensureOpaqueRuntime();
  return opaque_signin_start(
    textEncoder.encode(password),
  ) as OpaqueStartPayload;
};

/**
 * Finishes OPAQUE signin client flow.
 */
export const opaqueSigninFinish = async (
  flowId: string,
  password: string,
  opaqueServerMessage: Uint8Array,
): Promise<OpaqueFinishPayload> => {
  await ensureOpaqueRuntime();
  return opaque_signin_finish(
    flowId,
    textEncoder.encode(password),
    opaqueServerMessage,
  ) as OpaqueFinishPayload;
};

/**
 * Generates QR SVG markup locally in browser wasm runtime.
 */
export const generateQrSvg = async (payload: string): Promise<string> => {
  await ensureOpaqueRuntime();
  return generate_qr_svg(payload) as string;
};

export const generateWebDeviceIdentity = async (): Promise<WebDeviceIdentity> => {
  await ensureOpaqueRuntime();
  return generate_web_device_identity() as WebDeviceIdentity;
};

export const encryptVaultBytes = async (
  masterKey: Uint8Array,
  plaintext: Uint8Array,
): Promise<Uint8Array> => {
  await ensureOpaqueRuntime();
  return encrypt_vault_bytes(masterKey, plaintext);
};

export const decryptVaultBytes = async (
  masterKey: Uint8Array,
  encrypted: Uint8Array,
): Promise<Uint8Array> => {
  await ensureOpaqueRuntime();
  return decrypt_vault_bytes(masterKey, encrypted);
};

export const wrapAccountMasterKey = async (
  exportKey: Uint8Array,
  masterKey: Uint8Array,
): Promise<Uint8Array> => {
  await ensureOpaqueRuntime();
  return wrap_account_master_key(exportKey, masterKey);
};

export const unwrapAccountMasterKey = async (
  exportKey: Uint8Array,
  encryptedMasterKey: Uint8Array,
): Promise<Uint8Array> => {
  await ensureOpaqueRuntime();
  return unwrap_account_master_key(exportKey, encryptedMasterKey);
};

export const masterKeyToRecoveryPhrase = async (
  masterKey: Uint8Array,
): Promise<string> => {
  await ensureOpaqueRuntime();
  return master_key_to_recovery_phrase(masterKey);
};

export const recoveryPhraseToMasterKey = async (
  phrase: string,
): Promise<Uint8Array> => {
  await ensureOpaqueRuntime();
  return recovery_phrase_to_master_key(phrase);
};

export const wrapSpaceKeyForDevice = async (
  spaceKey: Uint8Array,
  hpkePublicKey: Uint8Array,
): Promise<unknown> => {
  await ensureOpaqueRuntime();
  return encrypt_group_key_for_peer(spaceKey, hpkePublicKey);
};

export const unwrapSpaceKeyForDevice = async (
  encryptedPackage: unknown,
  hpkePrivateKey: Uint8Array,
): Promise<Uint8Array> => {
  await ensureOpaqueRuntime();
  return decrypt_group_key_from_peer(encryptedPackage, hpkePrivateKey);
};

export const sealOperationEnvelope = async (input: {
  spaceId: string;
  streamId: string;
  clientOpId: string;
  authorDeviceId: string;
  keyEpoch: number;
  envelopeKind: "operation" | "snapshot" | "control";
  plaintext: Uint8Array;
  spaceKey: Uint8Array;
  signingPrivateKey: Uint8Array;
}): Promise<OperationEnvelopeV1> => {
  await ensureOpaqueRuntime();
  return seal_operation_envelope(
    input.spaceId,
    input.streamId,
    input.clientOpId,
    input.authorDeviceId,
    input.keyEpoch,
    input.envelopeKind,
    input.plaintext,
    input.spaceKey,
    input.signingPrivateKey,
  ) as OperationEnvelopeV1;
};

export const openOperationEnvelope = async (
  envelope: OperationEnvelopeV1,
  spaceKey: Uint8Array,
): Promise<Uint8Array> => {
  await ensureOpaqueRuntime();
  return open_operation_envelope(envelope, spaceKey);
};

export const verifyOperationEnvelope = async (
  envelope: OperationEnvelopeV1,
  signingPublicKey: Uint8Array,
): Promise<void> => {
  await ensureOpaqueRuntime();
  verify_operation_envelope(envelope, signingPublicKey);
};
