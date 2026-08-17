const base64UrlToBuffer = (input: string): ArrayBuffer => {
  const pad = "=".repeat((4 - (input.length % 4)) % 4);
  const binary = atob((input + pad).replace(/-/g, "+").replace(/_/g, "/"));
  return Uint8Array.from(binary, (character) => character.charCodeAt(0)).buffer;
};

const bufferToBase64Url = (buffer: ArrayBuffer): string => {
  const binary = Array.from(new Uint8Array(buffer), (byte) =>
    String.fromCharCode(byte),
  ).join("");
  return btoa(binary)
    .replace(/\+/g, "-")
    .replace(/\//g, "_")
    .replace(/=+$/g, "");
};

const toBuffer = (value: string | number[]): ArrayBuffer =>
  typeof value === "string"
    ? base64UrlToBuffer(value)
    : new Uint8Array(value).buffer;

const descriptor = (
  item: PublicKeyCredentialDescriptor,
): PublicKeyCredentialDescriptor => ({
  ...item,
  id:
    typeof (item.id as unknown) === "string" || Array.isArray(item.id)
      ? toBuffer(item.id as unknown as string | number[])
      : item.id,
});

export const parseCreationOptions = (
  raw: Uint8Array,
): PublicKeyCredentialCreationOptions => {
  const value = JSON.parse(new TextDecoder().decode(raw)) as
    PublicKeyCredentialCreationOptions & {
      challenge: string | number[];
      user: PublicKeyCredentialUserEntity & { id: string | number[] };
      excludeCredentials?: PublicKeyCredentialDescriptor[];
    };
  return {
    ...value,
    challenge: toBuffer(value.challenge),
    user: { ...value.user, id: toBuffer(value.user.id) },
    excludeCredentials: (value.excludeCredentials ?? []).map(descriptor),
  };
};

export const parseRequestOptions = (
  raw: Uint8Array,
): PublicKeyCredentialRequestOptions => {
  const value = JSON.parse(new TextDecoder().decode(raw)) as
    PublicKeyCredentialRequestOptions & {
      challenge: string | number[];
      allowCredentials?: PublicKeyCredentialDescriptor[];
    };
  return {
    ...value,
    challenge: toBuffer(value.challenge),
    allowCredentials: (value.allowCredentials ?? []).map(descriptor),
  };
};

export const serializeAttestation = (
  credential: PublicKeyCredential,
): Uint8Array => {
  const response = credential.response as AuthenticatorAttestationResponse;
  return new TextEncoder().encode(
    JSON.stringify({
      id: credential.id,
      rawId: bufferToBase64Url(credential.rawId),
      type: credential.type,
      response: {
        clientDataJSON: bufferToBase64Url(response.clientDataJSON),
        attestationObject: bufferToBase64Url(response.attestationObject),
        transports: response.getTransports?.(),
      },
      clientExtensionResults: credential.getClientExtensionResults(),
    }),
  );
};

export const serializeAssertion = (
  credential: PublicKeyCredential,
): Uint8Array => {
  const response = credential.response as AuthenticatorAssertionResponse;
  return new TextEncoder().encode(
    JSON.stringify({
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
    }),
  );
};
