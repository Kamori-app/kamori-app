/**
 * Normalizes binary values at JavaScript/WASM and legacy MessagePack boundaries.
 *
 * serde-wasm-bindgen serializes Rust fixed-size byte arrays as JavaScript arrays,
 * while byte buffers arrive as Uint8Array. Accept both representations, but
 * validate every element before cryptographic code consumes it.
 */
export const normalizeByteArray = (
  value: unknown,
  expectedLength: number,
  label: string,
): Uint8Array => {
  if (value instanceof Uint8Array) {
    if (value.length !== expectedLength) {
      throw new Error(`${label} must be ${expectedLength} bytes.`);
    }
    return value;
  }

  if (
    Array.isArray(value) &&
    value.length === expectedLength &&
    value.every(
      (entry) =>
        typeof entry === "number" &&
        Number.isInteger(entry) &&
        entry >= 0 &&
        entry <= 255,
    )
  ) {
    return Uint8Array.from(value);
  }

  throw new Error(`${label} must be ${expectedLength} bytes.`);
};
