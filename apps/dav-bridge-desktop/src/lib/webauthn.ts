const bytesToString = (bytes: number[]): string =>
  new TextDecoder().decode(new Uint8Array(bytes));

const base64UrlToBuffer = (input: string): ArrayBuffer => {
  const pad = "=".repeat((4 - (input.length % 4)) % 4);
  const base64 = (input + pad).replace(/-/g, "+").replace(/_/g, "/");
  const binary = atob(base64);
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i += 1) {
    bytes[i] = binary.charCodeAt(i);
  }
  return bytes.buffer;
};

const bufferToBase64Url = (buffer: ArrayBuffer): string => {
  const bytes = new Uint8Array(buffer);
  let binary = "";
  for (let i = 0; i < bytes.length; i += 1) {
    binary += String.fromCharCode(bytes[i]);
  }
  return btoa(binary)
    .replace(/\+/g, "-")
    .replace(/\//g, "_")
    .replace(/=+$/g, "");
};

const normalizeCredentialDescriptor = (
  descriptor: PublicKeyCredentialDescriptor,
): PublicKeyCredentialDescriptor => {
  const id = descriptor.id as unknown;
  if (typeof id === "string") {
    return { ...descriptor, id: base64UrlToBuffer(id) };
  }
  if (Array.isArray(id)) {
    return { ...descriptor, id: new Uint8Array(id).buffer };
  }
  return descriptor;
};

/**
 * Converts cloud `public_key_credential_request_options` bytes to WebAuthn request options.
 */
export const parseRequestOptions = (
  raw: number[],
): PublicKeyCredentialRequestOptions => {
  const json = JSON.parse(
    bytesToString(raw),
  ) as PublicKeyCredentialRequestOptions & {
    challenge: string | number[];
    allowCredentials?: PublicKeyCredentialDescriptor[];
  };

  const challengeRaw = json.challenge as unknown;
  const challenge =
    typeof challengeRaw === "string"
      ? base64UrlToBuffer(challengeRaw)
      : new Uint8Array(challengeRaw as number[]).buffer;
  const allowCredentials = json.allowCredentials?.map(
    normalizeCredentialDescriptor,
  );

  return {
    ...json,
    challenge,
    ...(allowCredentials ? { allowCredentials } : {}),
  };
};

/**
 * Serializes an assertion credential for cloud `/auth/passkey/login/finish`.
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

export const toUtf8Bytes = (value: string): number[] =>
  Array.from(new TextEncoder().encode(value));
