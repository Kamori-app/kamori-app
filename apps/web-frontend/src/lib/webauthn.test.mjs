import { describe, expect, test } from "bun:test";

import {
  buildDefaultPasskeyLabel,
  parseCreationOptions,
  parseRequestOptions,
} from "./webauthn.ts";

const encoded = (value) => new TextEncoder().encode(JSON.stringify(value));

const creationOptions = {
  challenge: "AQID",
  rp: { id: "kamori.app", name: "Kamori" },
  user: { id: "BAUG", name: "user", displayName: "user" },
  pubKeyCredParams: [{ alg: -7, type: "public-key" }],
};

describe("web WebAuthn option parsing", () => {
  test("parses the canonical options payload", () => {
    const result = parseCreationOptions(encoded(creationOptions));

    expect([...new Uint8Array(result.challenge)]).toEqual([1, 2, 3]);
    expect([...new Uint8Array(result.user.id)]).toEqual([4, 5, 6]);
  });

  test("accepts legacy creation and request envelopes", () => {
    const creation = parseCreationOptions(encoded({ publicKey: creationOptions }));
    const request = parseRequestOptions(
      encoded({ publicKey: { challenge: "AQID", allowCredentials: [] } }),
    );

    expect(creation.user.name).toBe("user");
    expect([...new Uint8Array(request.challenge)]).toEqual([1, 2, 3]);
  });

  test("builds a concise local authenticator label without the user agent", () => {
    const credential = {
      authenticatorAttachment: "platform",
      response: { getTransports: () => ["internal", "hybrid"] },
    };

    expect(buildDefaultPasskeyLabel(credential)).toBe(
      "Platform passkey · internal, hybrid",
    );
  });
});
