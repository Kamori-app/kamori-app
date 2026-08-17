export const REFRESH_TRANSPORT_HEADER = "X-Kamori-Refresh-Transport";
export const CSRF_HEADER = "X-Kamori-Csrf-Token";
export const DEFAULT_CSRF_COOKIE_NAME = "__Host-kamori_csrf";

/**
 * Reads a cookie value from a raw Cookie header/document.cookie string.
 */
export const readCookieValue = (cookieString, name) => {
  if (!cookieString || !name) {
    return null;
  }
  const encodedName = `${name}=`;
  for (const chunk of cookieString.split(";")) {
    const part = chunk.trim();
    if (!part.startsWith(encodedName)) {
      continue;
    }
    const value = part.slice(encodedName.length).trim();
    return value.length > 0 ? value : null;
  }
  return null;
};

/**
 * Builds cookie transport request options for refresh/logout endpoints.
 */
export const buildCookieRefreshTransportOptions = (
  cookieString,
  csrfCookieName = DEFAULT_CSRF_COOKIE_NAME,
) => {
  const csrfToken = readCookieValue(cookieString, csrfCookieName);
  return {
    headers: {
      [REFRESH_TRANSPORT_HEADER]: "cookie",
      ...(csrfToken ? { [CSRF_HEADER]: csrfToken } : {}),
    },
    credentials: "include",
  };
};
