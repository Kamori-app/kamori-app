import { decode, encode } from "@msgpack/msgpack";

export const PASSKEY_NAME_MAX_LENGTH = 120;

const PASSKEY_LABEL_CODEC = "kamori.passkey-label.v1";

type PasskeyLabelEnvelope = {
  codec: typeof PASSKEY_LABEL_CODEC;
  name: string;
};

const codePointLength = (value: string): number => Array.from(value).length;

/** Normalizes a user-visible passkey name before local encryption. */
export const normalizePasskeyName = (value: string): string => {
  const normalized = value.trim();
  if (!normalized) {
    throw new Error("Passkey name is required.");
  }
  if (codePointLength(normalized) > PASSKEY_NAME_MAX_LENGTH) {
    throw new Error(
      `Passkey name must be ${PASSKEY_NAME_MAX_LENGTH} characters or fewer.`,
    );
  }
  return normalized;
};

/** Encodes a domain-tagged plaintext envelope before vault encryption. */
export const encodePasskeyLabel = (name: string): Uint8Array =>
  encode({
    codec: PASSKEY_LABEL_CODEC,
    name: normalizePasskeyName(name),
  } satisfies PasskeyLabelEnvelope);

/**
 * Decodes the current envelope and accepts the former raw UTF-8 convention so
 * credentials created by early clients keep their names.
 */
export const decodePasskeyLabel = (plaintext: Uint8Array): string => {
  try {
    const envelope = decode(plaintext) as Partial<PasskeyLabelEnvelope>;
    if (
      envelope &&
      typeof envelope === "object" &&
      envelope.codec === PASSKEY_LABEL_CODEC &&
      typeof envelope.name === "string"
    ) {
      return normalizePasskeyName(envelope.name);
    }
  } catch {
    // Try the pre-envelope UTF-8 representation below.
  }

  const legacyName = new TextDecoder("utf-8", { fatal: true }).decode(plaintext);
  return normalizePasskeyName(legacyName);
};
