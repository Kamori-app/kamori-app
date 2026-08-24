import { describe, expect, test } from "bun:test";

import { parseCreationOptions, parseRequestOptions } from "./webauthn.ts";

const encoded = (value) => new TextEncoder().encode(JSON.stringify(value));

const creationOptions = {
  challenge: "AQID",
  rp: { id: "kamori.app", name: "Kamori Admin" },
  user: { id: "BAUG", name: "operator", displayName: "operator" },
  pubKeyCredParams: [{ alg: -7, type: "public-key" }],
};

describe("operator WebAuthn option parsing", () => {
  test("parses the canonical creation-options payload", () => {
    const result = parseCreationOptions(encoded(creationOptions));

    expect([...new Uint8Array(result.challenge)]).toEqual([1, 2, 3]);
    expect([...new Uint8Array(result.user.id)]).toEqual([4, 5, 6]);
  });

  test("accepts the legacy webauthn-rs creation envelope", () => {
    const result = parseCreationOptions(encoded({ publicKey: creationOptions }));

    expect(result.user.name).toBe("operator");
  });

  test("accepts the legacy webauthn-rs request envelope", () => {
    const result = parseRequestOptions(
      encoded({ publicKey: { challenge: "AQID", allowCredentials: [] } }),
    );

    expect([...new Uint8Array(result.challenge)]).toEqual([1, 2, 3]);
  });

  test("reports an incomplete creation payload explicitly", () => {
    expect(() => parseCreationOptions(encoded({ challenge: "AQID" }))).toThrow(
      "incomplete WebAuthn creation options",
    );
  });
});
