import { afterEach, expect, mock, test } from "bun:test";
import { decode, encode } from "@msgpack/msgpack";
import { runWithAccessRefreshRetry } from "./session-flow.js";
const attempts = new Map;
let serial = 0;
mock.module("./refresh-attempt.js", () => ({
  loadOrCreateRefreshAttempt: async (base, csrf) => {
    const scope = `${base}/${csrf}`;
    if (!attempts.has(scope))
      attempts.set(scope, `request-${++serial}`);
    return { scope, requestId: attempts.get(scope) };
  },
  clearRefreshAttempt: async (scope) => {
    attempts.delete(scope);
  }
}));
const { cloudApi } = await import("../api/cloud");
const originalFetch = globalThis.fetch;
const originalNavigator = Object.getOwnPropertyDescriptor(globalThis, "navigator");
afterEach(() => {
  globalThis.fetch = originalFetch;
  if (originalNavigator)
    Object.defineProperty(globalThis, "navigator", originalNavigator);
  else
    Reflect.deleteProperty(globalThis, "navigator");
  attempts.clear();
});
function browser() {
  const base = `https://tab-${++serial}.example`;
  let cookie = "A";
  let lockHeld = false;
  let tail = Promise.resolve();
  const requests = [];
  let failure = "none";
  Object.defineProperty(globalThis, "navigator", { configurable: true, value: { locks: {
    request: async (_name, callback) => {
      const previous = tail;
      let release;
      tail = new Promise((resolve) => {
        release = resolve;
      });
      await previous;
      lockHeld = true;
      try {
        return await callback();
      } finally {
        lockHeld = false;
        release();
      }
    }
  } } });
  globalThis.fetch = async (url, options) => {
    const path = String(url).slice(base.length);
    const reply = (body, status = 200) => new Response(encode(body), {
      status,
      headers: { "content-type": "application/msgpack" }
    });
    if (path === "/auth/signin/finish")
      return reply({ csrf_token: cookie });
    if (path === "/auth/csrf") {
      expect(lockHeld).toBe(true);
      return reply({ csrf_token: cookie });
    }
    expect(path).toBe("/auth/refresh");
    const csrf = new Headers(options?.headers).get("X-Kamori-Csrf-Token");
    const body = decode(options?.body);
    requests.push({ csrf, id: body.rotation_request_id });
    if (csrf !== cookie || failure === "revoked")
      return reply({ message: "revoked or csrf mismatch" }, 401);
    const outcome = failure;
    failure = "none";
    if (outcome === "lost")
      throw new TypeError("lost response");
    cookie = String.fromCharCode(cookie.charCodeAt(0) + 1);
    if (outcome === "partial")
      throw new TypeError("lost body after Set-Cookie");
    return reply({ access_token: `access-${cookie}`, csrf_token: cookie });
  };
  return { base, requests, rotateOtherTab: () => {
    cookie = "B";
  }, fail: (value) => {
    failure = value;
  } };
}
const warm = (base) => cloudApi.signinFinish(base, {});
test("shared-cookie rotation bootstraps current CSRF inside the refresh lock", async () => {
  const b = browser();
  await warm(b.base);
  b.rotateOtherTab();
  expect((await cloudApi.refresh(b.base)).access_token).toBe("access-C");
  expect(b.requests.map((r) => r.csrf)).toEqual(["B"]);
});
test("concurrent same-tab callers share one refresh result", async () => {
  const b = browser();
  await warm(b.base);
  const responses = await Promise.all([cloudApi.refresh(b.base), cloudApi.refresh(b.base)]);
  expect(responses.map((r) => r.access_token)).toEqual(["access-B", "access-B"]);
  expect(b.requests).toHaveLength(1);
});
for (const failure of ["lost", "partial"]) {
  test(`${failure} response retry uses the durable request id for the current cookie generation`, async () => {
    const b = browser();
    await warm(b.base);
    b.fail(failure);
    await expect(cloudApi.refresh(b.base)).rejects.toThrow("lost");
    await cloudApi.refresh(b.base);
    expect(b.requests).toHaveLength(2);
    expect(b.requests[1].csrf).toBe(failure === "lost" ? "A" : "B");
    expect(b.requests[1].id === b.requests[0].id).toBe(failure === "lost");
  });
}
test("genuinely revoked refresh still clears authentication", async () => {
  const b = browser();
  await warm(b.base);
  b.fail("revoked");
  let cleared = false;
  await expect(runWithAccessRefreshRetry({
    getAccessToken: () => "expired",
    operation: async () => {
      throw { status: 401 };
    },
    refresh: () => cloudApi.refresh(b.base),
    onRefreshUnauthorized: () => {
      cleared = true;
    }
  })).rejects.toThrow("Session expired");
  expect(cleared).toBe(true);
});

test('independent tabs without Web Locks fail before bootstrap and preserve authentication', async () => {
  const first = (await import('../api/cloud.ts?no-locks-first')).cloudApi;
  const second = (await import('../api/cloud.ts?no-locks-second')).cloudApi;
  expect(first).not.toBe(second);
  Object.defineProperty(globalThis, 'navigator', { configurable: true, value: {} });
  const emitted = [];
  globalThis.fetch = async (url) => {
    emitted.push(String(url));
    return new Response(encode(String(url).endsWith('/auth/csrf')
      ? { csrf_token: 'shared-A' }
      : { csrf_token: 'shared-B', access_token: 'rotated' }), { status: 200 });
  };
  let cleared = 0;
  const results = await Promise.allSettled([first, second].map(client => runWithAccessRefreshRetry({
    getAccessToken: () => 'current-access',
    operation: async () => { throw { status: 401 }; },
    refresh: () => client.refresh('https://shared.example'),
    onRefreshUnauthorized: () => { cleared++; },
  })));
  expect(results.map(result => result.status)).toEqual(['rejected', 'rejected']);
  for (const result of results) {
    expect(result.reason.message).toContain('coordination');
    expect(result.reason.status).not.toBe(401);
  }
  expect(cleared).toBe(0);
  expect(emitted).toEqual([]);
});
