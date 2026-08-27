/**
 * WebAuthn conversion helpers between backend JSON payloads and browser APIs.
 */
const base64UrlToBuffer = (input: string): ArrayBuffer => {
  const pad = "=".repeat((4 - (input.length % 4)) % 4);
  const base64 = (input + pad).replace(/-/g, "+").replace(/_/g, "/");
  const binary = atob(base64);
  const bytes = new Uint8Array(binary.length);
  for (let index = 0; index < binary.length; index += 1) {
    bytes[index] = binary.charCodeAt(index);
  }
  return bytes.buffer;
};

const bufferToBase64Url = (buffer: ArrayBuffer): string => {
  const bytes = new Uint8Array(buffer);
  let binary = "";
  for (let index = 0; index < bytes.length; index += 1) {
    binary += String.fromCharCode(bytes[index]);
  }
  return btoa(binary)
    .replace(/\+/g, "-")
    .replace(/\//g, "_")
    .replace(/=+$/g, "");
};

const toArrayBuffer = (value: string | number[]): ArrayBuffer => {
  if (typeof value === "string") {
    return base64UrlToBuffer(value);
  }
  return new Uint8Array(value).buffer;
};

/**
 * Normalizes descriptor IDs into `ArrayBuffer` values required by browser API.
 */
const normalizeDescriptor = (
  descriptor: PublicKeyCredentialDescriptor,
): PublicKeyCredentialDescriptor => {
  const id = descriptor.id as unknown;
  if (typeof id === "string" || Array.isArray(id)) {
    return {
      ...descriptor,
      id: toArrayBuffer(id as string | number[]),
    };
  }
  return descriptor;
};

type JsonObject = Record<string, unknown>;

const asObject = (value: unknown, context: string): JsonObject => {
  if (typeof value !== "object" || value === null || Array.isArray(value)) {
    throw new Error(`Server returned invalid ${context}.`);
  }
  return value as JsonObject;
};

const unwrapPublicKey = (value: unknown, context: string): JsonObject => {
  const envelope = asObject(value, context);
  return "publicKey" in envelope
    ? asObject(envelope.publicKey, context)
    : envelope;
};

/**
 * Converts cloud passkey add options bytes to browser WebAuthn options.
 */
export const parseCreationOptions = (
  raw: Uint8Array,
): PublicKeyCredentialCreationOptions => {
  const json = unwrapPublicKey(
    JSON.parse(new TextDecoder().decode(raw)),
    "WebAuthn creation options",
  ) as unknown as PublicKeyCredentialCreationOptions & {
    challenge: string | number[];
    user: { id: string | number[]; name: string; displayName: string };
    excludeCredentials?: PublicKeyCredentialDescriptor[];
  };
  if (json.challenge === undefined || json.user?.id === undefined) {
    throw new Error("Server returned incomplete WebAuthn creation options.");
  }

  return {
    ...json,
    challenge: toArrayBuffer(json.challenge),
    user: {
      ...json.user,
      id: toArrayBuffer(json.user.id),
    },
    excludeCredentials: (json.excludeCredentials ?? []).map(
      normalizeDescriptor,
    ),
  };
};

/**
 * Converts cloud passkey login options bytes to browser WebAuthn options.
 */
export const parseRequestOptions = (
  raw: Uint8Array,
): PublicKeyCredentialRequestOptions => {
  const json = unwrapPublicKey(
    JSON.parse(new TextDecoder().decode(raw)),
    "WebAuthn request options",
  ) as unknown as PublicKeyCredentialRequestOptions & {
    challenge: string | number[];
    allowCredentials?: PublicKeyCredentialDescriptor[];
  };
  if (json.challenge === undefined) {
    throw new Error("Server returned incomplete WebAuthn request options.");
  }
  const allowCredentials = json.allowCredentials?.map(normalizeDescriptor);

  return {
    ...json,
    challenge: toArrayBuffer(json.challenge),
    ...(allowCredentials ? { allowCredentials } : {}),
  };
};

/**
 * Serializes a registration credential for `/auth/passkey/add/finish`.
 */
export const serializeAttestationCredential = (
  credential: PublicKeyCredential,
): Record<string, unknown> => {
  const response = credential.response as AuthenticatorAttestationResponse;

  return {
    id: credential.id,
    rawId: bufferToBase64Url(credential.rawId),
    type: credential.type,
    response: {
      clientDataJSON: bufferToBase64Url(response.clientDataJSON),
      attestationObject: bufferToBase64Url(response.attestationObject),
      transports:
        typeof response.getTransports === "function"
          ? response.getTransports()
          : undefined,
    },
    clientExtensionResults: credential.getClientExtensionResults(),
  };
};

/**
 * Serializes a login credential for `/auth/passkey/login/finish`.
 */
export const serializeAssertionCredential = (
  credential: PublicKeyCredential,
): Record<string, unknown> => {
  const response = credential.response as AuthenticatorAssertionResponse;

  return {
    id: credential.id,
    rawId: bufferToBase64Url(credential.rawId),
    type: credential.type,
    response: {
      clientDataJSON: bufferToBase64Url(response.clientDataJSON),
      authenticatorData: bufferToBase64Url(response.authenticatorData),
      signature: bufferToBase64Url(response.signature),
      userHandle: response.userHandle
        ? bufferToBase64Url(response.userHandle)
        : null,
    },
    clientExtensionResults: credential.getClientExtensionResults(),
  };
};

/**
 * Builds a default passkey label from local browser/authenticator hints.
 * This function is client-only and should run before encrypting/storing the label.
 */
export const buildDefaultPasskeyLabel = (
  credential: PublicKeyCredential,
): string => {
  const attachment =
    credential.authenticatorAttachment === "platform"
      ? "Platform passkey"
      : credential.authenticatorAttachment === "cross-platform"
        ? "External passkey"
        : "Passkey";
  const response = credential.response as AuthenticatorAttestationResponse;
  const transports =
    typeof response.getTransports === "function"
      ? response.getTransports().join(", ")
      : "";
  return transports ? `${attachment} · ${transports}` : attachment;
};

/**
 * Encodes text payload as UTF-8 bytes for MessagePack transport.
 */
export const toUtf8Bytes = (value: string): Uint8Array =>
  new TextEncoder().encode(value);
