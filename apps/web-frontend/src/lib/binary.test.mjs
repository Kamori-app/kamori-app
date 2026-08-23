import { describe, expect, test } from "bun:test";

import { normalizeByteArray } from "./binary.ts";

describe("binary boundary normalization", () => {
  test("accepts a byte buffer with the required length", () => {
    const input = new Uint8Array([1, 2, 3]);
    expect(normalizeByteArray(input, 3, "key")).toBe(input);
  });

  test("normalizes a legacy fixed-array representation", () => {
    expect(normalizeByteArray([1, 2, 3], 3, "key")).toEqual(
      new Uint8Array([1, 2, 3]),
    );
  });

  test("rejects invalid bytes instead of coercing them", () => {
    expect(() => normalizeByteArray([1, -1, 256], 3, "key")).toThrow(
      "key must be 3 bytes",
    );
    expect(() => normalizeByteArray([1, 2.5, 3], 3, "key")).toThrow(
      "key must be 3 bytes",
    );
  });
});
