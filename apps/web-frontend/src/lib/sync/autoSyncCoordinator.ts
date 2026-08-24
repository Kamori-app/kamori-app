export type SyncReason =
  | "initial"
  | "interval"
  | "online"
  | "visible"
  | "focus"
  | "local-change"
  | "manual";

export interface AutoSyncOptions {
  run: (reason: SyncReason) => Promise<boolean>;
  ready: () => boolean;
  intervalMs?: number;
  debounceMs?: number;
  maxBackoffMs?: number;
}

/**
 * Coordinates browser sync triggers without allowing overlapping runs.
 * Data-plane mutual exclusion across tabs remains the responsibility of the
 * supplied `run` callback (Kamori uses the Web Locks API there).
 */
export class AutoSyncCoordinator {
  readonly #run: AutoSyncOptions["run"];
  readonly #ready: AutoSyncOptions["ready"];
  readonly #intervalMs: number;
  readonly #debounceMs: number;
  readonly #maxBackoffMs: number;
  #window: Window | null = null;
  #document: Document | null = null;
  #interval: number | null = null;
  #scheduled: number | null = null;
  #running = false;
  #rerun = false;
  #failures = 0;

  constructor(options: AutoSyncOptions) {
    this.#run = options.run;
    this.#ready = options.ready;
    this.#intervalMs = options.intervalMs ?? 30_000;
    this.#debounceMs = options.debounceMs ?? 250;
    this.#maxBackoffMs = options.maxBackoffMs ?? 5 * 60_000;
  }

  start(windowObject: Window, documentObject: Document): void {
    if (this.#window) return;
    this.#window = windowObject;
    this.#document = documentObject;
    windowObject.addEventListener("online", this.#onOnline);
    windowObject.addEventListener("focus", this.#onFocus);
    documentObject.addEventListener("visibilitychange", this.#onVisibility);
    this.#interval = windowObject.setInterval(() => {
      if (documentObject.visibilityState === "visible") {
        this.request("interval");
      }
    }, this.#intervalMs);
    this.request("initial", true);
  }

  stop(): void {
    if (!this.#window || !this.#document) return;
    this.#window.removeEventListener("online", this.#onOnline);
    this.#window.removeEventListener("focus", this.#onFocus);
    this.#document.removeEventListener("visibilitychange", this.#onVisibility);
    if (this.#interval !== null) this.#window.clearInterval(this.#interval);
    if (this.#scheduled !== null) this.#window.clearTimeout(this.#scheduled);
    this.#window = null;
    this.#document = null;
    this.#interval = null;
    this.#scheduled = null;
  }

  request(reason: SyncReason, immediate = false): void {
    if (!this.#window || !this.#ready()) return;
    if (this.#running) {
      this.#rerun = true;
      return;
    }
    if (this.#scheduled !== null) this.#window.clearTimeout(this.#scheduled);
    const backoff = this.#failures === 0
      ? 0
      : Math.min(this.#maxBackoffMs, 2_000 * 2 ** (this.#failures - 1));
    const jitter = backoff === 0 ? 0 : Math.floor(Math.random() * Math.min(1_000, backoff / 4));
    const delay = immediate ? 0 : Math.max(this.#debounceMs, backoff + jitter);
    this.#scheduled = this.#window.setTimeout(() => {
      this.#scheduled = null;
      void this.#execute(reason);
    }, delay);
  }

  async #execute(reason: SyncReason): Promise<void> {
    if (!this.#ready()) return;
    this.#running = true;
    try {
      const succeeded = await this.#run(reason);
      this.#failures = succeeded ? 0 : Math.min(this.#failures + 1, 12);
    } catch {
      this.#failures = Math.min(this.#failures + 1, 12);
    } finally {
      this.#running = false;
      if (this.#rerun) {
        this.#rerun = false;
        this.request("local-change");
      }
    }
  }

  #onOnline = () => this.request("online", true);
  #onFocus = () => this.request("focus");
  #onVisibility = () => {
    if (this.#document?.visibilityState === "visible") {
      this.request("visible");
    }
  };
}
