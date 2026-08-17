import { test } from "bun:test";
import assert from "node:assert/strict";

import {
  bestEffortLogoutWithRefresh,
  runWithAccessRefreshRetry,
} from "./session-flow.js";

test("runWithAccessRefreshRetry executes operation without refresh on success", async () => {
  let refreshCalls = 0;
  const result = await runWithAccessRefreshRetry({
    getAccessToken: () => "access-1",
    operation: async (token) => {
      assert.equal(token, "access-1");
      return "ok";
    },
    refresh: async () => {
      refreshCalls += 1;
      return { access_token: "rotated" };
    },
  });

  assert.equal(result, "ok");
  assert.equal(refreshCalls, 0);
});

test("runWithAccessRefreshRetry refreshes and retries once on 401", async () => {
  let operationCalls = 0;
  let rotatedToken = null;

  const result = await runWithAccessRefreshRetry({
    getAccessToken: () => "expired-token",
    operation: async (token) => {
      operationCalls += 1;
      if (operationCalls === 1) {
        assert.equal(token, "expired-token");
        throw { status: 401 };
      }
      assert.equal(token, "new-token");
      return "retried";
    },
    refresh: async () => ({ access_token: "new-token" }),
    onAccessTokenRotated: (token) => {
      rotatedToken = token;
    },
  });

  assert.equal(result, "retried");
  assert.equal(operationCalls, 2);
  assert.equal(rotatedToken, "new-token");
});

test("runWithAccessRefreshRetry clears auth path on refresh 401", async () => {
  let unauthorizedCalls = 0;

  await assert.rejects(
    () =>
      runWithAccessRefreshRetry({
        getAccessToken: () => "expired-token",
        operation: async () => {
          throw { status: 401 };
        },
        refresh: async () => {
          throw { status: 401 };
        },
        onRefreshUnauthorized: () => {
          unauthorizedCalls += 1;
        },
      }),
    (error) => {
      assert.equal(error instanceof Error, true);
      assert.equal(error.message, "Session expired. Sign in again.");
      return true;
    },
  );

  assert.equal(unauthorizedCalls, 1);
});

test("bestEffortLogoutWithRefresh does nothing without access token", async () => {
  let logoutCalls = 0;
  let refreshCalls = 0;

  await bestEffortLogoutWithRefresh({
    accessToken: null,
    logout: async () => {
      logoutCalls += 1;
    },
    refresh: async () => {
      refreshCalls += 1;
      return { access_token: "unused" };
    },
  });

  assert.equal(logoutCalls, 0);
  assert.equal(refreshCalls, 0);
});

test("bestEffortLogoutWithRefresh retries logout after refresh on 401", async () => {
  const logoutTokens = [];
  let rotatedToken = null;

  await bestEffortLogoutWithRefresh({
    accessToken: "expired-token",
    logout: async (token) => {
      logoutTokens.push(token);
      if (logoutTokens.length === 1) {
        throw { status: 401 };
      }
    },
    refresh: async () => ({ access_token: "rotated-token" }),
    onAccessTokenRotated: (token) => {
      rotatedToken = token;
    },
  });

  assert.deepEqual(logoutTokens, ["expired-token", "rotated-token"]);
  assert.equal(rotatedToken, "rotated-token");
});

test("bestEffortLogoutWithRefresh does not refresh on non-401 logout error", async () => {
  let refreshCalls = 0;

  await bestEffortLogoutWithRefresh({
    accessToken: "access-token",
    logout: async () => {
      throw { status: 500 };
    },
    refresh: async () => {
      refreshCalls += 1;
      return { access_token: "rotated-token" };
    },
  });

  assert.equal(refreshCalls, 0);
});
