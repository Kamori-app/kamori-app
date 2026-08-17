import { xchacha20poly1305 } from "@noble/ciphers/chacha.js";

/**
 * Client-side invite-code utilities:
 * normalization, hashing, generation, and XChaCha20-Poly1305 wrapping helpers.
 */
const INVITE_CODE_GROUPS = 4;
const INVITE_CODE_GROUP_SIZE = 4;
const INVITE_CODE_LENGTH = INVITE_CODE_GROUPS * INVITE_CODE_GROUP_SIZE;
const INVITE_CODE_ALPHABET = "ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
const INVITE_NONCE_SIZE = 24;
const COLLECTION_KEY_SIZE = 32;
const INVITE_HASH_DOMAIN = "kamori:invite:lookup:v1";
const INVITE_KEY_DOMAIN = "kamori:invite:key:v1";

const textEncoder = new TextEncoder();

/**
 * Computes SHA-256(domain || 0x00 || normalized_invite_code).
 */
const domainSeparatedDigest = async (
  domain: string,
  normalizedInviteCode: string,
): Promise<Uint8Array> => {
  const domainBytes = textEncoder.encode(domain);
  const codeBytes = textEncoder.encode(normalizedInviteCode);
  const input = new Uint8Array(domainBytes.length + 1 + codeBytes.length);
  input.set(domainBytes, 0);
  input[domainBytes.length] = 0;
  input.set(codeBytes, domainBytes.length + 1);

  const keyMaterial = await crypto.subtle.digest("SHA-256", input);
  return new Uint8Array(keyMaterial);
};

/**
 * Computes lookup hash used by server-side invite storage.
 */
const deriveInviteLookupHash = async (
  normalizedInviteCode: string,
): Promise<Uint8Array> =>
  domainSeparatedDigest(INVITE_HASH_DOMAIN, normalizedInviteCode);

/**
 * Derives a 32-byte symmetric key from normalized invite code.
 */
const deriveInviteKey = async (
  normalizedInviteCode: string,
): Promise<Uint8Array> =>
  domainSeparatedDigest(INVITE_KEY_DOMAIN, normalizedInviteCode);

/**
 * Converts invite code to canonical `A-Z0-9` uppercase form without separators.
 */
export const normalizeInviteCode = (inviteCode: string): string | null => {
  const normalized = inviteCode.toUpperCase().replace(/[^A-Z0-9]/g, "");

  return normalized.length === INVITE_CODE_LENGTH ? normalized : null;
};

/**
 * Computes domain-separated SHA-256 lookup hash of normalized invite code.
 */
export const hashInviteCode = async (
  inviteCode: string,
): Promise<Uint8Array> => {
  const normalizedInviteCode = normalizeInviteCode(inviteCode);
  if (!normalizedInviteCode) {
    throw new Error("invite code format is invalid");
  }

  return deriveInviteLookupHash(normalizedInviteCode);
};

/**
 * Generates a human-shareable invite code like `ABCD-EFGH-JKLM-NPQR`.
 */
export const generateInviteCode = (): string => {
  let raw = "";
  for (let index = 0; index < INVITE_CODE_LENGTH; index += 1) {
    const randomIndex =
      crypto.getRandomValues(new Uint32Array(1))[0] %
      INVITE_CODE_ALPHABET.length;
    raw += INVITE_CODE_ALPHABET[randomIndex];
  }

  const chunks: string[] = [];
  for (let index = 0; index < raw.length; index += INVITE_CODE_GROUP_SIZE) {
    chunks.push(raw.slice(index, index + INVITE_CODE_GROUP_SIZE));
  }
  return chunks.join("-");
};

/**
 * Encrypts arbitrary bytes with invite-code-derived XChaCha20-Poly1305 key.
 *
 * Payload format:
 * - first 24 bytes: nonce
 * - remaining bytes: ciphertext + tag
 */
export const wrapBytesWithInviteCode = async (
  plaintext: Uint8Array,
  inviteCode: string,
): Promise<Uint8Array> => {
  const normalizedInviteCode = normalizeInviteCode(inviteCode);
  if (!normalizedInviteCode) {
    throw new Error("invite code format is invalid");
  }

  const key = await deriveInviteKey(normalizedInviteCode);
  const nonce = crypto.getRandomValues(new Uint8Array(INVITE_NONCE_SIZE));
  const ciphertext = xchacha20poly1305(key, nonce).encrypt(plaintext);

  const payload = new Uint8Array(nonce.length + ciphertext.length);
  payload.set(nonce, 0);
  payload.set(ciphertext, nonce.length);
  return payload;
};

/**
 * Decrypts invite payload back into raw bytes.
 */
export const unwrapBytesWithInviteCode = async (
  encryptedPayload: Uint8Array,
  inviteCode: string,
): Promise<Uint8Array> => {
  if (encryptedPayload.length <= INVITE_NONCE_SIZE) {
    throw new Error("encrypted invite payload is malformed");
  }

  const normalizedInviteCode = normalizeInviteCode(inviteCode);
  if (!normalizedInviteCode) {
    throw new Error("invite code format is invalid");
  }

  const key = await deriveInviteKey(normalizedInviteCode);
  const nonce = encryptedPayload.slice(0, INVITE_NONCE_SIZE);
  const ciphertext = encryptedPayload.slice(INVITE_NONCE_SIZE);

  let plaintext: Uint8Array;
  try {
    plaintext = xchacha20poly1305(key, nonce).decrypt(ciphertext);
  } catch {
    throw new Error("failed to decrypt invite payload");
  }

  return plaintext;
};

/**
 * Encrypts collection key bytes with invite-code-derived XChaCha20-Poly1305 key.
 */
export const wrapCollectionKeyWithInviteCode = async (
  collectionKey: Uint8Array,
  inviteCode: string,
): Promise<Uint8Array> => {
  if (collectionKey.length !== COLLECTION_KEY_SIZE) {
    throw new Error("collection key must be 32 bytes");
  }
  return wrapBytesWithInviteCode(collectionKey, inviteCode);
};

/**
 * Decrypts invite payload back into 32-byte collection key.
 */
export const unwrapCollectionKeyWithInviteCode = async (
  encryptedGroupKey: Uint8Array,
  inviteCode: string,
): Promise<Uint8Array> => {
  const plaintext = await unwrapBytesWithInviteCode(
    encryptedGroupKey,
    inviteCode,
  );
  if (plaintext.length !== COLLECTION_KEY_SIZE) {
    throw new Error("decrypted invite payload has invalid key size");
  }
  return plaintext;
};
