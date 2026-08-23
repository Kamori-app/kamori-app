import { describe, expect, test } from "bun:test";
import { normalizeCloudBaseUrl } from "./endpoint.ts";

describe("cloud endpoint normalization", () => {
  test("accepts HTTPS and strips trailing slashes", () => {
    expect(normalizeCloudBaseUrl(" https://api.kamori.app/// ")).toBe(
      "https://api.kamori.app",
    );
  });

  test("allows cleartext HTTP only on loopback", () => {
    expect(normalizeCloudBaseUrl("http://127.0.0.1:3000/")).toBe(
      "http://127.0.0.1:3000",
    );
    expect(() => normalizeCloudBaseUrl("http://api.kamori.app")).toThrow(
      "must use HTTPS",
    );
  });

  test("rejects credential and ambiguous URL suffixes", () => {
    expect(() => normalizeCloudBaseUrl("https://user@api.kamori.app")).toThrow();
    expect(() => normalizeCloudBaseUrl("https://api.kamori.app/api")).toThrow(
      "without a path",
    );
    expect(() => normalizeCloudBaseUrl("https://api.kamori.app?tenant=x")).toThrow();
    expect(() => normalizeCloudBaseUrl("https://api.kamori.app/#api")).toThrow();
  });
});
