/**
 * True when error object carries HTTP 401 status.
 */
export const isUnauthorizedError = (error) =>
  Boolean(
    error &&
      typeof error === "object" &&
      "status" in error &&
      Number(error.status) === 401,
  );

/**
 * Runs an authenticated operation with one refresh retry on 401.
 */
export const runWithAccessRefreshRetry = async ({
  getAccessToken,
  operation,
  refresh,
  onAccessTokenRotated,
  onRefreshUnauthorized,
  missingAccessTokenMessage = "Sign in first.",
  refreshUnauthorizedMessage = "Session expired. Sign in again.",
}) => {
  const accessToken = getAccessToken?.();
  if (!accessToken) {
    throw new Error(missingAccessTokenMessage);
  }

  try {
    return await operation(accessToken);
  } catch (error) {
    if (!isUnauthorizedError(error)) {
      throw error;
    }
  }

  try {
    const rotated = await refresh();
    const rotatedAccessToken =
      typeof rotated?.access_token === "string"
        ? rotated.access_token.trim()
        : "";
    if (!rotatedAccessToken) {
      throw new Error("refresh did not return access token");
    }

    onAccessTokenRotated?.(rotatedAccessToken);
    return await operation(rotatedAccessToken);
  } catch (refreshError) {
    if (isUnauthorizedError(refreshError)) {
      onRefreshUnauthorized?.();
      throw new Error(refreshUnauthorizedMessage);
    }
    throw refreshError;
  }
};

/**
 * Performs best-effort logout and retries once via refresh when logout returns 401.
 */
export const bestEffortLogoutWithRefresh = async ({
  accessToken,
  logout,
  refresh,
  onAccessTokenRotated,
}) => {
  if (!accessToken) {
    return;
  }

  try {
    await logout(accessToken);
    return;
  } catch (error) {
    if (!isUnauthorizedError(error)) {
      return;
    }
  }

  try {
    const rotated = await refresh();
    const rotatedAccessToken =
      typeof rotated?.access_token === "string"
        ? rotated.access_token.trim()
        : "";
    if (!rotatedAccessToken) {
      return;
    }

    onAccessTokenRotated?.(rotatedAccessToken);
    await logout(rotatedAccessToken);
  } catch {
    // best-effort: ignore follow-up logout errors
  }
};
