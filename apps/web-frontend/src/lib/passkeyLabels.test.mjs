import { describe, expect, test } from "bun:test";

import {
  PASSKEY_NAME_MAX_LENGTH,
  decodePasskeyLabel,
  encodePasskeyLabel,
  normalizePasskeyName,
} from "./passkeyLabels.ts";

describe("passkey labels", () => {
  test("round-trips the domain-tagged label envelope", () => {
    expect(decodePasskeyLabel(encodePasskeyLabel("  MacBook · Bitwarden  "))).toBe(
      "MacBook · Bitwarden",
    );
  });

  test("accepts legacy UTF-8 labels", () => {
    expect(decodePasskeyLabel(new TextEncoder().encode("Security key"))).toBe(
      "Security key",
    );
  });

  test("rejects empty and oversized names by Unicode character count", () => {
    expect(() => normalizePasskeyName("   ")).toThrow("required");
    expect(() => normalizePasskeyName("🔑".repeat(PASSKEY_NAME_MAX_LENGTH))).not.toThrow();
    expect(() =>
      normalizePasskeyName("🔑".repeat(PASSKEY_NAME_MAX_LENGTH + 1)),
    ).toThrow("120 characters or fewer");
  });
});
