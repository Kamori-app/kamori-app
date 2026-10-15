### Spec Compliance

- ✅ The requested mounted behavioral coverage is implemented: desktop `test` retains typechecking and invokes the desktop/web suite (`apps/dav-bridge-desktop/package.json:12`, `:16`); admin retains checking and existing units and invokes its mounted suite (`apps/admin-frontend/package.json:13`, `:14`). The seven cases comprise three desktop, two separately labelled production web expiry cases, and two admin cases. Ruling 10 permits the web cases; no desktop sharing feature was added.
- ✅ Desktop coverage exercises production sign-in-only controls, rejected credentials and recovery, restored/offline dashboard controls and collection validation with visible DOM and exact IPC assertions (`tests/acceptance/specs/desktop-ui.spec.ts:39`, `:66`, `:94`).
- ✅ Admin coverage exercises cancelled/deferred passkeys, rejected TOTP and recovery, confirmation/fresh-ceremony barriers, pending mutation, conflict and successful refresh with decoded MessagePack and authenticator assertions (`tests/acceptance/specs/admin-ui.spec.ts:70`, `:99`).
- ✅ Web TTL coverage selects 15 and 10080 minutes in the mounted production sharing view, verifies the complete visible option set, checks actual invite/rotation fields, independently hashes the displayed invite code, and asserts binary encrypted payloads (`tests/acceptance/specs/invite-ui.spec.ts:11`, `:49`, `:62`).
- ✅ Product scope is limited to the three-line confirmation guard (`apps/admin-frontend/src/routes/+page.svelte:162`). Other cases add coverage of existing behavior. No dependency changes, test switches, native changes, branch/ref changes or unrelated feature work appear in the supplied diff.
- ⚠️ The new default test commands lack browser provisioning in the existing frontend CI job; this is an Important integration defect below. Local passing runs do not establish fresh-runner viability.
- ⚠️ Real server authorization, physical authenticators, packaged Tauri OS behavior and preservation of earlier fixes beyond the supplied diff are outside this synthetic-boundary task gate. No end-to-end production-environment claim is made.

### Strengths

- Production components and business orchestration execute directly: desktop mounts App and substitutes only official Tauri mockIPC (`tests/acceptance/fixtures/desktopUi.js:2`); admin mounts its actual route (`tests/acceptance/fixtures/adminUi.js:2`); sharing seeds a synthetic unlocked account with production vault/crypto helpers and mounts AppWorkspace (`tests/acceptance/fixtures/inviteUi.js:4`, `:10`, `:25`). The bundler compiles imported Svelte source and resolves the production libraries rather than providing replacement validation/encryption logic (`tests/acceptance/fixtures/uiGates.bundle.mjs:11`, `:22`, `:34`).
- The R06 regression is meaningful and correctly classified as a client confirmation/pre-ceremony defect. The unchanged strict assertion requires a visible confirmation error and zero reauth/privileged requests (`tests/acceptance/specs/admin-ui.spec.ts:108`). The saved RED log records the empty-confirmation request before failing that assertion; GREEN logs pass the same case. The real backend independently rejects a mismatched confirmation before consuming reauthentication (`apps/cloud-server/src/features/admin/services.rs:700`), so this is not evidence of a backend authorization bypass.
- Pending-boundary tests hold the authenticator and HTTP/IPC response to inspect real disabled controls and prohibit premature writes (`tests/acceptance/specs/admin-ui.spec.ts:124`, `tests/acceptance/specs/desktop-ui.spec.ts:50`). Errors and retries verify visible recovery rather than rendering snapshots.
- Cache inputs include the external tests, fixtures, helper and config, plus web source/package inputs including generated WASM (`turbo.json:55`, `:72`). Saved `ui-turbo-dry.json` confirms six external admin inputs, 74 external desktop inputs, retained owning-package defaults and root `bun.lock`/package configuration. Its input blob hashes agree with the supplied diff for all new task files. The global lock hash covers external dependency changes.

### Issues

#### Critical (Must Fix)

- None found.

#### Important (Should Fix)

- `apps/admin-frontend/package.json:14`, `apps/dav-bridge-desktop/package.json:16`, `.github/workflows/ci.yml:163`: both default package tests now launch Playwright Chromium, but the frontend job only installs Bun dependencies before running them (`.github/workflows/ci.yml:164`). Playwright's browser archive and Linux runtime dependencies are not installed by `bun install`; the browser installation at `.github/workflows/ci.yml:348` belongs to a separate acceptance job and cannot provision this runner. A fresh frontend CI runner will fail the new gates at browser launch. Add a browser provisioning step before Test frontends, reusing the existing workspace-pinned `bun run --cwd tests/acceptance playwright install --with-deps chromium` command. Validate the workflow wiring and affected-path detection; the already-passing UI suites need not be repeated for a workflow-only correction.

#### Minor (Nice to Have)

- `/private/tmp/kamori-readiness-20261015/ui-admin-build.log`: adapter-node emits `Generated an empty chunk: "chunks/env.js".` The report discloses it and the build exits 0. This is output noise, not a failed check or a demonstrated regression from this task; keep it visible in final evidence rather than describing all output as pristine.

### Checks and Review Scope

- Read the saved 658-line review diff once. Its tool output truncated two admin-test assertions; read only that missing diff segment to recover the no-write/deferred-reauth assertions. The production updateSetting hunk ended mid-function, so read its surrounding script to verify reauthentication ordering, error handling and busy cleanup. No other changed source file was reread.
- Named risk: R06 could be mistaken for a backend authorization defect. Focused unchanged-code check: `apps/cloud-server/src/features/admin/services.rs:691–715`; exact confirmation validation already exists before token consumption.
- Named risk: new browser-based default tests may be unprovisioned or skipped in CI. Focused checks: frontend/acceptance browser steps in `.github/workflows/ci.yml`, its detector invocation, the frontend/server path cases in `scripts/ci/detect-affected.sh:69–96`, and `tests/acceptance/playwright.config.ts:6`. Frontend provisioning is missing as reported; acceptance paths still trigger the server acceptance job, whose default testDir includes the new specs.
- Inspected existing saved RED, final desktop/admin check/test/build logs and Turbo dry-run JSON. Final evidence shows both checks with 0 errors/0 warnings, desktop 5/5 UI (3 desktop + 2 web), admin 2/2 UI + 4/4 existing units, and both builds exiting 0 on Bun 1.3.14. No suites or focused tests were rerun, no git commands were used, and no source/index/ref changes were made during review.

### Assessment

**Task quality:** Needs fixes.

**Reasoning:** The mounted suites exercise real production behavior at meaningful synthetic boundaries, and the small client guard has genuine RED → GREEN evidence. Add Chromium provisioning to the existing frontend CI job before these new default package gates can be relied on in a fresh environment.
