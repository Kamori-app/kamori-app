import { describe, expect, test } from "bun:test";
import {
  buildRecoveryKitFilename,
  buildRecoveryKitText,
} from "./recoveryKitFile.ts";

const words = Array.from({ length: 24 }, (_, index) => `word${index + 1}`);

describe("downloadable data recovery kit", () => {
  test("numbers all 24 words and documents the local plaintext boundary", () => {
    const text = buildRecoveryKitText("alice", words.join(" "));

    expect(text).toContain("Account / Аккаунт: alice");
    expect(text).toContain("1. word1");
    expect(text).toContain("24. word24");
    expect(text).toContain("created locally in your browser");
    expect(text).toContain("незашифрованный файл создан локально");
  });

  test("rejects incomplete phrases instead of producing a misleading backup", () => {
    expect(() => buildRecoveryKitText("alice", words.slice(0, 23).join(" "))).toThrow(
      "exactly 24 words",
    );
  });

  test("uses an opaque recognizable filename without account metadata", () => {
    const filename = buildRecoveryKitFilename(
      Uint8Array.from([0, 1, 2, 3, 4, 5, 254, 255]),
    );

    expect(filename).toBe("kamori-recovery-000102030405feff.txt");
    expect(filename).not.toContain("alice");
  });
});
