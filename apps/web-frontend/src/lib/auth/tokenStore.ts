/**
 * In-memory-only token store for web runtime.
 *
 * Tokens are intentionally not persisted in local/session storage.
 */
let accessToken: string | null = null;

const normalizeToken = (value: string | null | undefined): string | null => {
  if (!value) {
    return null;
  }
  const trimmed = value.trim();
  return trimmed.length > 0 ? trimmed : null;
};

export const tokenStore = {
  setAccessToken(access: string) {
    accessToken = normalizeToken(access);
  },

  clear() {
    accessToken = null;
  },

  getAccessToken(): string | null {
    return accessToken;
  },
};
