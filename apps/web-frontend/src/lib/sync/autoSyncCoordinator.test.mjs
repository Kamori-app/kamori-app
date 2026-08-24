import { describe, expect, test } from "bun:test";
import { AutoSyncCoordinator } from "./autoSyncCoordinator.ts";

class FakeWindow extends EventTarget {
  setTimeout = globalThis.setTimeout.bind(globalThis);
  clearTimeout = globalThis.clearTimeout.bind(globalThis);
  setInterval = globalThis.setInterval.bind(globalThis);
  clearInterval = globalThis.clearInterval.bind(globalThis);
}

class FakeDocument extends EventTarget {
  visibilityState = "visible";
}

const waitFor = async (predicate) => {
  for (let attempt = 0; attempt < 100; attempt += 1) {
    if (predicate()) return;
    await new Promise((resolve) => setTimeout(resolve, 2));
  }
  throw new Error("condition was not reached");
};

describe("AutoSyncCoordinator", () => {
  test("runs an initial sync and stops browser triggers", async () => {
    let runs = 0;
    const coordinator = new AutoSyncCoordinator({
      ready: () => true,
      run: async () => {
        runs += 1;
        return true;
      },
      intervalMs: 60_000,
      debounceMs: 1,
    });
    const fakeWindow = new FakeWindow();
    const fakeDocument = new FakeDocument();

    coordinator.start(fakeWindow, fakeDocument);
    await waitFor(() => runs === 1);
    coordinator.stop();
    fakeWindow.dispatchEvent(new Event("online"));
    await new Promise((resolve) => setTimeout(resolve, 5));

    expect(runs).toBe(1);
  });

  test("coalesces requests received during an active sync", async () => {
    let runs = 0;
    let active = 0;
    let maximumActive = 0;
    let releaseFirst;
    const firstRun = new Promise((resolve) => {
      releaseFirst = resolve;
    });
    const coordinator = new AutoSyncCoordinator({
      ready: () => true,
      run: async () => {
        runs += 1;
        active += 1;
        maximumActive = Math.max(maximumActive, active);
        if (runs === 1) await firstRun;
        active -= 1;
        return true;
      },
      intervalMs: 60_000,
      debounceMs: 1,
    });
    const fakeWindow = new FakeWindow();
    const fakeDocument = new FakeDocument();

    coordinator.start(fakeWindow, fakeDocument);
    await waitFor(() => runs === 1);
    coordinator.request("local-change", true);
    coordinator.request("focus", true);
    releaseFirst();
    await waitFor(() => runs === 2);
    coordinator.stop();

    expect(runs).toBe(2);
    expect(maximumActive).toBe(1);
  });
});
