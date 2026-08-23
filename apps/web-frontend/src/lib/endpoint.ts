/** Normalizes a cloud origin while preventing credentials and cleartext remote APIs. */
export function normalizeCloudBaseUrl(value: string): string {
  const parsed = new URL(value.trim());
  const isLoopback =
    parsed.hostname === "localhost" ||
    parsed.hostname === "127.0.0.1" ||
    parsed.hostname === "[::1]" ||
    parsed.hostname === "::1";
  if (parsed.protocol !== "https:" && !(parsed.protocol === "http:" && isLoopback)) {
    throw new Error("Remote Kamori endpoints must use HTTPS.");
  }
  if (parsed.username || parsed.password || parsed.search || parsed.hash) {
    throw new Error("Kamori endpoint must not contain credentials, query, or fragment.");
  }
  if (!/^\/*$/.test(parsed.pathname)) {
    throw new Error("Kamori endpoint must be an origin without a path.");
  }
  parsed.pathname = "";
  return parsed.toString().replace(/\/$/, "");
}
