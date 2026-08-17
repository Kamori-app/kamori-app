import { test } from "bun:test";
import assert from "node:assert/strict";

import {
  computeScrollTarget,
  computeTopOffset,
  pickActiveSection,
} from "./section-nav.js";

/**
 * Unit coverage for sticky-nav section activation helpers.
 */
test("computeTopOffset uses minimum floor", () => {
  assert.equal(computeTopOffset(0), 96);
  assert.equal(computeTopOffset(40), 96);
});

test("computeTopOffset tracks nav height when larger than floor", () => {
  assert.equal(computeTopOffset(100), 124);
  assert.equal(computeTopOffset(137.6), 162);
});

test("pickActiveSection returns null for empty section list", () => {
  assert.equal(pickActiveSection([], 0, 120), null);
});

test("pickActiveSection keeps first section active near top", () => {
  const sections = [
    { id: "why-kamori", absTop: 200 },
    { id: "what-kamori-is", absTop: 700 },
    { id: "how-it-works", absTop: 1200 },
  ];
  assert.equal(pickActiveSection(sections, 0, 120), "why-kamori");
});

test("pickActiveSection switches only after crossing section threshold", () => {
  const sections = [
    { id: "why-kamori", absTop: 200 },
    { id: "what-kamori-is", absTop: 700 },
    { id: "how-it-works", absTop: 1200 },
    { id: "downloads", absTop: 1700 },
    { id: "security", absTop: 2200 },
    { id: "sharing", absTop: 2800 },
    { id: "faq", absTop: 3400 },
  ];

  assert.equal(pickActiveSection(sections, 1078, 120), "what-kamori-is");
  assert.equal(pickActiveSection(sections, 1079, 120), "how-it-works");

  assert.equal(pickActiveSection(sections, 2078, 120), "downloads");
  assert.equal(pickActiveSection(sections, 2079, 120), "security");

  assert.equal(pickActiveSection(sections, 2678, 120), "security");
  assert.equal(pickActiveSection(sections, 2679, 120), "sharing");
});

test("computeScrollTarget applies sticky nav and extra offset", () => {
  assert.equal(computeScrollTarget(2000, 100, 20), 1880);
  assert.equal(computeScrollTarget(980, 56), 904);
});
