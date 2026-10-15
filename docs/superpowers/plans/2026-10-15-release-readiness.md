# Kamori remaining audit and release-readiness implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Close the locally actionable test, native-build, interoperability and operational-validation gaps remaining after the thirty-defect audit, fixing every newly demonstrated defect and retaining honest external-device limits.

**Architecture:** Preserve the current signed encrypted operation log, client cache/refresh state machines, transaction boundaries and opt-in projection. Add behavioral evidence at real persistence/native/UI boundaries; make minimal production fixes only when a reproducible failure demonstrates a violated existing contract. Use synthetic disposable resources throughout.

**Tech Stack:** Rust 1.97.1, Bun 1.3.14, PostgreSQL18 for application tests / PostgreSQL16 for hosted restore, Flutter 3.47, Xcode 27/iOS SDK27 with existing iOS26.2 simulator runtime, Playwright, pgBackRest, MinIO and Go1.25.11.

**Spec:** `SPEC.md` (especially sections 6–9,11,13), `AGENTS.md`, `docs/audits/2026-10-15-business-tests.md`, `docs/audits/2026-10-15-decisions.md`, and the user's accepted six-item remaining-work list. Baseline: `dc13f1909cc9acff3e13d8fe849a26943efe17ce`.

## Global Constraints

- Work in the existing local branch `fix/audit-remediation`, repository `/Users/yujin/Development/kamori-app/kamori-app/kamori-app`. Do not create another branch/worktree or modify the restored `codex/pause-hosted-infrastructure` ref. User explicitly prohibits new `codex/` branches.
- Registration is web-portal only. Desktop and mobile clients are sign-in-only. Sharing uses invite codes; expiry is 15 minutes through 7 days.
- API transport remains MessagePack with human-readable Serde UUID strings and binary byte fields. The canonical cloud model is a signed encrypted operation log.
- Desktop cache: `.kamori/local-cache.sqlite3`; mobile: `.kamori/mobile-cache.sqlite3`. Preserve encrypted PIM, outbox, monotonic cursors and generation-specific refresh retry identity.
- Mobile platforms remain Android/iOS only. Real FRB is mandatory for native/integration builds (`KAMORI_USE_MOCK_RUST=false`). No mobile localhost DAV server; projection is explicit opt-in and background must never request new permissions.
- Preserve all thirty reviewed fixes, including account gate913 admission, space→quota731→ordered account locking, maintenance single-transaction boundaries and native in-flight lease ownership. Do not remove coordination to improve timings.
- No production services, real credentials, private PKI or SECRETS.md reads, public deployment, push, signing submission, legal-term acceptance or privileged system changes. Use only dedicated test accounts, schemas, containers and newly created simulator data.
- Existing Xcode license has now been accepted by the user. SDK27 and Xcode27 preflight pass; installed simulator runtime iOS26.2 is available outside sandbox. Do not download a new runtime without demonstrating necessity.
- Pin toolchains. Rust: PATH=/Users/yujin/.cargo/bin:$PATH, rustup run 1.97.1 cargo; reuse repository `target` cache. Bun binary directory: /Users/yujin/.bun/install/cache/@oven/bun-darwin-aarch64@1.3.14@@@1/bin. Flutter: /opt/homebrew/share/flutter/bin/flutter. Set DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer for Apple tools.
- Parent owns shared local infrastructure, final integration runs and docs/ledger. Runtime artifacts: `/private/tmp/kamori-readiness-20261015`. Shared synthetic PostgreSQL URL will be supplied before server tasks. Workers never clean shared infrastructure, never spawn agents and commit only their task's implementation/tests.
- Each production change requires meaningful RED→GREEN evidence. New tests that already pass are coverage evidence, not invented defects. Compile failures may be build RED; do not manufacture a failure by deleting correct code. No weakened assertions, arbitrary retries or timeout increases to conceal failures.
- No broad dependency upgrades, unrelated refactors, advanced recurrence editor, new auth protocol, per-session runtime architecture or private-CA support. A required small test dependency is allowed with a lockfile update and explanation.

## Review Focus

- Missing DB configuration must never count skipped integration bodies as passed; Task2 tests the actual filtered test executable with both DB variables absent.
- Reset racing refresh or passkey commit must not leave post-reset live capabilities; Task3 stages both transaction orders and inspects final database state.
- A process/tab crash after persistence and before acknowledgement must not lose outbox data, rewind cursors or replay unsupported credentials; Task4 reopens actual storage and uses genuine browser contexts.
- Imported recurrence exceptions and unknown fields must survive unrelated edits and DAV round-trips; Task6 uses a shared concrete corpus across exposed boundaries without promising a new editor.
- Native builds and backup scripts must execute their real dependencies; Tasks1/8 distinguish successful unsigned/simulator/local-restore evidence from signing, hardware scheduling and remote disaster-recovery certification.

### Task 1: Build iOS with the real Rust bridge and exercise native boundaries

**Files:** `apps/dav-bridge-mobile/ios/Runner/AppDelegate.swift`, `Info.plist`, `Runner.xcodeproj/project.pbxproj`, `RunnerTests/RunnerTests.swift`; mobile `pubspec.yaml`/lock only for an SDK integration-test dependency if necessary; new `integration_test/native_services_test.dart` and a narrow reusable native-test runner under `scripts/` if needed; `lib/src/services/system_projection_service.dart` and focused Flutter tests for a demonstrated native missing-record contract mismatch. Existing generated Apple files may change only when required by the current Flutter/Xcode toolchain.
**Interfaces:** Existing `kamori/background-sync` channel method `isScheduled`, scheduling identifier `kamori_background_sync_task`, production Calendar/Contacts adapters, real FRB factory. No wire or auth/session API changes.

- [x] Run `flutter analyze`/existing tests as baseline, then `DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer PATH=/Users/yujin/.cargo/bin:$PATH RUSTUP_TOOLCHAIN=1.97.1 flutter build ios --release --no-codesign --dart-define=KAMORI_USE_MOCK_RUST=false` from mobile. Save full log; investigate the first real build failure with systematic debugging.
- [x] Fix actual compile/link/plugin-registration failures minimally; retain real packaged core and generated FRB alignment. Verify device arm64 binary links the real core. Do not accept licenses, change global xcode-select or use flutter create.
- [x] Replace empty `testExample` with meaningful native configuration/registration tests. Add a simulator smoke against real FRB and available native channels; use a newly created dedicated simulator and synthetic records only. Exercise Contacts/Calendar create→read/update→delete where simulator/provider permissions allow and the background admission channel's real return. Native tests must clean only IDs they created. Reproduce production reconciliation after an externally deleted projected contact, plus disable/remove when its ID is already absent; repair only proven missing-record handling while preserving genuine provider/permission errors and iOS note omission.
- [x] Build/test on existing iOS26.2 simulator runtime if possible; distinguish a simulator limitation from a failure of product code. Record scheduler delivery, physical secure store/power loss and signed IPA as unproven when hardware/credentials are unavailable. A real installed plugin/channel is required for claims about native behavior.
- [x] Run mobile analyze/test once on final source, unsigned device build, applicable simulator/native tests, diff check; commit task files and report full commands, outcomes and any exact external block.

### Task 2: Make missing PostgreSQL test configuration fail explicitly

**Files:** existing `apps/cloud-server/src/platform/test_support.rs` (extend current test_config/test_state helpers without replacing them); `platform/mod.rs` only if needed; DB helpers/callers in auth/handlers, admin, invites and operations integration tests, including operations/lock_regressions.rs if it shares the helper; `apps/cloud-server/package.json`, `turbo.json`, root testing docs/CI only when required to expose the intended test command.
**Interfaces:** A shared test-only `required_test_database_url() -> String` resolves nonempty KAMORI_DATABASE_URL then DATABASE_URL, otherwise fails with an actionable missing-PostgreSQL message. No environment mutation in concurrently running tests.

- [x] Add a subprocess regression proving a representative real DB integration test exits nonzero with both DB variables absent; old behavior must produce RED because it exits successfully without executing assertions.
- [x] Centralize required URL resolution and remove silent missing-DB success paths across all current cloud-server DB helpers. Keep normal unit-only filtered commands possible and explain the full suite's DB prerequisite; preserve existing migrations/schema isolation.
- [x] Cover explicit URL, fallback URL and absent/empty inputs through isolated subprocesses or a pure resolver; do not log credentials or modify global environment unsafely.
- [x] Run no-DB regression plus full server suite against supplied dedicated PostgreSQL, strict Clippy/fmt and appropriate CI detector/shell checks if touched. Commit and report exact counts; no-DB failure must be intentional and documented.

- [x] Controller follow-up: make the documented root Turbo server-test path retain the required DB configuration in strict env and execute real DB tests instead of cached success; verify the actual one-test supplied-URL path, absent-URL failure, fallback and repeat-run behavior. Initial helper review is complete; this boundary was discovered during Task4 and is queued immediately afterward without parallel implementers.

### Task 3: Prove reset versus refresh and deferred passkey commit ordering

**Files:** auth/handlers integration tests or focused sibling test module; auth repositories/services only when the new production-path regression proves a defect. Reuse Task2 DB prerequisite helper.
**Interfaces:** Existing AuthenticationBinding/auth_generation, refresh session rows and passkey registration persistence. Preserve MessagePack, exact refresh retry and one-use authorization semantics.

- [x] Add deterministic `refresh_commit_and_password_reset_serialize_in_both_orders`: real refresh/reset transaction paths, observed PostgreSQL lock wait, reset-first rejects stale capability, refresh-first replacement is revoked by the subsequent reset. Assert no surviving unauthorized session and no unrelated-account mutation.
- [x] Add `passkey_registration_commit_rejects_reset_generation`: prepare a valid deferred registration with a software/test authenticator or existing valid fixture, pause the actual commit transaction versus reset, and assert no stale passkey row/capability. Cover commit-first then reset behavior according to the retained generation/session contract.
- [x] Add controls for an eligible current-generation completion and exact retry where supported. Do not substitute a mocked final binding validator for production persistence or require physical authenticator hardware.
- [x] If any assertion exposes a real violation, record actual RED and minimally fix it; otherwise report these as newly proven coverage. Run focused cases, full server once, strict Clippy/fmt; commit and report.

### Task 4: Exercise real client crash durability and multi-tab coordination

**Files:** core local_bridge_runner cache/tests or test-only sibling module; web cryptoVault/auth refresh-attempt implementation and AppWorkspace materialization only for demonstrated durability/account-scope defects; new `tests/acceptance/specs/browser-durability.spec.ts`, synthetic fixture/helper changes as needed.
**Interfaces:** Existing scoped SQLCipher RefreshCredentialStore, durable pending refresh attempt, IndexedDB outbox/materialization/cursors, browser Web Locks and shared cookies. No new production test hooks or unsafe refresh fallback.

- [x] Add child-process crash/reopen tests using actual encrypted SQLite: committed pending attempt survives abrupt child exit; committed local PIM/outbox remains after reopening; applied server cursor is monotonic; unsupported old secure token never reaches network. Distinguish process termination from physical power-loss certification.
- [x] Add genuine Playwright two-page/same-context tests for shared-cookie refresh serialization and lost/partial refresh response. Assert exact request identity/number and preserve unrelated session; use real Web Locks, browser storage and production client orchestration.
- [x] Add IndexedDB durability/account-scope tests across page close/reopen and tab concurrency: acknowledged local edits remain queued/materialized, sync acknowledgement is idempotent, account switching never exposes another account's cache. Use synthetic accounts and real browser storage, not fake IndexedDB.
- [x] Run focused native/browser cases; apply only demonstrated minimal fixes with RED→GREEN evidence. Run affected core FRB/desktop or web checks/units/builds once, regenerate bindings only if an unavoidable API change, commit and report.

### Task 5: Add behavioral desktop and admin UI gates

**Files:** desktop package scripts and new focused UI tests/fixtures under desktop or tests/acceptance; existing desktop LoginPage/CollectionsPage/DashboardPage only for proven behavior defects; admin UI tests/fixtures and components only when proven. Reuse pinned Playwright and existing Svelte compilation infrastructure.
**Interfaces:** Actual mounted Svelte components with synthetic Tauri IPC or HTTP/WebAuthn boundaries. Production IPC dispatch, data validation and UI state must run; no production-only testing switch.

- [ ] Make desktop `test` run a behavioral suite in addition to its existing typecheck. Test sign-in-only UI and error recovery, restored/offline control state, collection action validation and invite expiry bounds without replacing component logic with a fixture implementation.
- [ ] Test admin authentication error/second-factor states and at least one privileged mutation's pending/success/failure behavior with actual mounted UI and synthetic transport/authenticator boundaries. Ensure no privileged request before the required ceremony/confirmation.
- [ ] Assert visible DOM changes and exact boundary calls/payloads; mocked OS/network boundaries are acceptable but static source-string tests and mere rendering snapshots are insufficient.
- [ ] Fix only demonstrated regressions; run desktop/admin check/test/build on Bun1.3.14 and report real behavioral counts separately from typecheck. Commit task files and any strictly required test-only dependency/lockfile change.

### Task 6: Verify imported recurrence/unknown-field/DAV compatibility

**Files:** shared fixture corpus under `tests/fixtures/pim/`; core parser/materializer/DAV conformance tests; web pim tests and mobile pim_models tests, production projection/recurrence code only for a demonstrated violation of existing supported behavior.
**Interfaces:** Existing version2 typed temporals and lossless iCalendar/vCard projection. SPEC7 requires unknown properties/recurrence exceptions survive partial edits; advanced exception editing remains post-MVP.

- [ ] Add fixtures with RRULE+EXDATE+RDATE, a detached RECURRENCE-ID override, TZID crossing DST, all-day values, custom property and non-Kamori alarm. Assert unrelated title/note edits preserve exact unmanaged values/exception components.
- [ ] Exercise import→canonical materialization→first-party partial edit→DAV GET/PUT using real core/HTTP boundaries. Assert stable UID/stream/parent and no silent exception loss, duplicated override or orphan outbox operation.
- [ ] Pin currently supported occurrence-display behavior in web/mobile against the corpus. Trace any mismatch to an existing contract before treating it as a bug; report unsupported advanced authoring explicitly rather than silently implementing a new editor or changing time policy.
- [ ] For real failures, record RED and minimally fix. Run core FRB/DAV, affected web check/units in UTC/Tbilisi/Los Angeles and Flutter analyze/test if touched; commit and report interoperability limits honestly.

### Task 7: Measure contention and validate bounded production lock behavior

**Files:** focused server concurrency workload tests or reusable `tests/release-readiness/` runner; existing operation/ownership/deletion/maintenance code only for reproduced defects; native lease tests and a concise benchmark result artifact.
**Interfaces:** Existing account gate913, quota731, sorted row prelocks, process-global mobile lease. Do not replace them with weaker synchronization or change deletion atomicity.

- [ ] Run a finite, reproducible multi-account/multi-space workload of real append/blob/ownership/admission/delete paths at up to16 workers. Track transaction outcomes, exact final usage, unique monotonic sequences and zero deadlocks/internal errors; expected authorization/conflict outcomes must be explicit. Include a named late-invite-redemption versus recipient deletion schedule after membership cleanup, checking whether current production authorization/persistence can create an active membership for a successfully deleted account; classify only from the real contract and outcome.
- [ ] Exercise a representative large-account operation-usage repair with existing migration locking; record dataset size, elapsed time and concurrent writer waiting. Do not impose a timing SLA without a product requirement or run against production.
- [ ] Measure queued foreground/background lease behavior with a deliberately slow synthetic native/network boundary; verify bounded existing HTTP timeout behavior and no permit leak after failure/owner disposal. Record foreground waiting as an accepted cost, not a new per-session-runtime mandate.
- [ ] If measurements expose deadlock, leak, incorrect quota or an actually unbounded path, add a deterministic regression before minimal fix. Run affected suites/Clippy/fmt; commit reusable tests and report environment/sample sizes/latency distribution and limits.

### Task 8: Execute disposable pgBackRest PITR and blob restore exercises

**Files:** reusable local-only restore harness under `tests/release-readiness/restore/`, supporting container definitions; `docs/runbooks/backup-restore.md` and evidence; production backup/replication scripts only for a reproduced orchestration defect.
**Interfaces:** Real PostgreSQL16 matching hosted provisioning/runbook, pgBackRest encrypted repository/WAL archive/restore, real MinIO primary/DR endpoints and production blob replication tooling. Parent provisions exact own container/project names; no production endpoints/credentials or host PGDATA.

- [ ] Create a synthetic migrated database with known operation/counter/account canaries; create an encrypted pgBackRest full backup and archive WAL across two distinguishable committed markers. Use the actual backup job/archive check where locally adaptable, with only external paths/env configured for the sandbox.
- [ ] Restore to a point between markers into a distinct disposable PGDATA/container, start it, and assert schema/checksum/accounting and pre-target marker/ciphertext bytes match while post-target marker is absent. Record backup/check/restore commands and targeted recovery outcome.
- [ ] Put synthetic ciphertext objects in the local primary store, execute actual replication, restore/read from independent DR and compare size/hash. Prove missing/corrupt data is detected, not merely successful process exit.
- [ ] Fix any demonstrated script/runbook errors minimally and add behavioral regression. Keep destructive cleanup strictly scoped to owned project/volume labels; never read actual secrets or claim this proves remote-provider/production disaster recovery.
- [ ] Run affected Go1.25.11 tests/vet/shell checks, archive concise local restore evidence, commit harness/docs/fixes and report exact objects/rows/target and cleanup result.

## Controller final integration and completion

- [ ] Preserve separate per-task base/head commits, reports and spec+quality reviews. Scope rereviews to fix diffs; no duplicate full suites to confirm a report.
- [ ] Run final pinned workspace Rust+FRB with supplied PostgreSQL, fmt/strict Clippy, web/admin/desktop checks/tests/builds, Flutter analyze/tests and real native builds affected by changed source. Run full fresh-image browser acceptance after source is frozen.
- [ ] Record only freshly measured coverage or retain exact prior revision provenance; test-only lines/expanded populations cannot prove full business coverage.
- [ ] Save new findings, actual RED/GREEN, benchmark/restore/native results and remaining physical/signing/legal gates in `docs/audits/2026-10-15-readiness.md`; update the existing business-test matrix without erasing historical baseline evidence.
- [ ] Perform one final review of this continuation range, fix all confirmed findings, collect every new Ruling with its cost into permanent docs/final response, archive reports, and remove only this plan's scratch/resources after clean review. Keep `fix/audit-remediation` checked out; no merge/push or branch creation.
