import { test } from "bun:test";
import assert from "node:assert/strict";

import {
  CSRF_HEADER,
  DEFAULT_CSRF_COOKIE_NAME,
  REFRESH_TRANSPORT_HEADER,
  buildCookieRefreshTransportOptions,
  readCookieValue,
} from "./cookie-csrf.js";

test("readCookieValue extracts cookie by name", () => {
  const cookie = "a=1; __Host-kamori_csrf=csrf-token; z=3";
  assert.equal(readCookieValue(cookie, DEFAULT_CSRF_COOKIE_NAME), "csrf-token");
});

test("readCookieValue returns null when absent", () => {
  const cookie = "a=1; b=2";
  assert.equal(readCookieValue(cookie, DEFAULT_CSRF_COOKIE_NAME), null);
});

test("buildCookieRefreshTransportOptions always sets cookie transport", () => {
  const options = buildCookieRefreshTransportOptions("");
  assert.equal(options.credentials, "include");
  assert.equal(options.headers[REFRESH_TRANSPORT_HEADER], "cookie");
});

test("buildCookieRefreshTransportOptions includes csrf header when cookie exists", () => {
  const options = buildCookieRefreshTransportOptions(
    "x=1; __Host-kamori_csrf=csrf-123",
  );
  assert.equal(options.headers[CSRF_HEADER], "csrf-123");
});

test("buildCookieRefreshTransportOptions omits csrf header when cookie is missing", () => {
  const options = buildCookieRefreshTransportOptions("x=1; y=2");
  assert.equal(options.headers[CSRF_HEADER], undefined);
});
