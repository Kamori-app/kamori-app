# Kamori audit remediation implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task. Steps use checkbox syntax for tracking.

**Goal:** Fix every confirmed defect S01–S07, C01–C05, U01–U08, I01–I02 and T01 from the repository audit, with regression evidence and independent reviews.

**Architecture:** Keep existing feature/service/repository boundaries, encrypted oplog model, durable native credential/cache machinery and platform adapters. Repair violated invariants at their authoritative boundary; avoid unrelated rewrites or new product scope.

**Tech Stack:** Rust 1.97.1 / Axum / PostgreSQL / SQLCipher / Tauri, Bun 1.3.14 / Svelte, Flutter, Go / Pulumi, Bash.

**Spec:** `SPEC.md`, `AGENTS.md`, and `docs/audits/2026-10-15-kamori.md` plus the four audit evidence appendices.

## Global Constraints

- Registration is web-portal only. Desktop and mobile clients are sign-in-only.
- Sharing uses short-lived invite codes (generate + redeem), not direct member id input in UI.
- Invite expiry is user-selectable from 15 minutes through 7 days.
- All active application endpoints use MessagePack with human-readable Serde semantics for cross-language types. UUIDs are canonical lowercase hyphenated strings; binary protocol values stay MessagePack `bin` values rather than JSON number arrays.
- Desktop: `.kamori/local-cache.sqlite3`; Mobile: `.kamori/mobile-cache.sqlite3`.
- Mobile target platforms are Android + iOS only. Mobile clients do not run a localhost DAV server.
- The canonical cloud model is a signed encrypted operation log. DAV is a local desktop projection, not the server data model.
- Production and integration builds must use the real packaged Rust bridge and must never fall back to the mock runtime.
- Every client persists a random per-generation rotation request id before sending a refresh. An exact retry returns the same still-active replacement for its lifetime; a different reuse id revokes the account's refresh sessions.
- System calendar/contact projection requires explicit opt-in per collection; background work must not prompt for new permission.
- New SQL migrations must have unique numeric prefixes; do not edit committed old migrations.
- Work exclusively in the isolated worktree, use synthetic data. Do not deploy, push, access production, alter real credentials, or read SECRETS.md/private PKI.
- Add behavioral regression tests for corrected business logic; record RED/GREEN evidence. Test doubles are allowed for OS/keychain/network boundaries; assertions must exercise real orchestration.
- Use `PATH=/Users/yujin/.cargo/bin:$PATH rustup run 1.97.1 cargo` (PATH cargo/rustc/Clippy are newer), `CARGO_TARGET_DIR=/private/tmp/kamori-audit-target-197`, and the controller's disposable DB URL. Reuse controller dependencies and avoid unrelated lockfile changes.

## Review Focus

- Reset/revocation racing deferred authentication must not create a fresh authorized session; tested by Task 1.
- Lost-response retries must retain exact authorization/credential generation, not revive revoked entitlement; tested by Tasks 1, 2, 4, 6.
- Concurrent writers and cascade deletes must preserve atomic quotas/cursors; tested by Task 2.
- Existing random-ID/conflict DAV resources must preserve their stream and parent before enqueue; tested by Task 3.
- Partial PIM edits and recurrence must preserve dates, instants and DST semantics across client zones; tested by Task 5.

### Task 1: Invalidate deferred authentication after reset or revocation

**Files:** auth services/signin.rs, device_authorization.rs, passkeys/reauth/enrollment deferred grant paths as necessary; auth repositories/users.rs, refresh_tokens.rs, account_recovery_attempts.rs and exports; platform/security/opaque.rs; auth HTTP integration tests; new migration `202610150001_authentication_generation.sql` if a durable generation is used.
**Interfaces:** Consumes existing HTTP payloads and sessions. Produces transactionally validated authorization binding; wire payloads remain compatible.
**Requirements:** Resolve S04/S05 in server audit. Bind OPAQUE login exchanges and TOTP continuations to the credential state used to authenticate, and approved device flows to that state and their approving live session. Prefer durable `users.auth_generation BIGINT NOT NULL DEFAULT 0` incremented atomically with password/recovery changes; an equivalent exact credential snapshot is acceptable only if it closes the same transactional race. Validate the expected binding while holding the same user/session locks used to insert the new refresh session. Reject stale flows after recovery, password change and approving-session revocation; do not merely compare in a service before a separate insert. Audit neighboring deferred authentication completions for the same race (passkey and reauth/enrollment grants) and carry the binding consistently. Existing active exact device-token retries remain idempotent, revoked replacements remain rejected. No key material returned before all factors pass.

- [x] Add HTTP/DB regressions: `opaque_login_started_before_recovery_is_rejected`, `opaque_login_started_before_password_change_is_rejected`, `device_approval_before_recovery_is_rejected`, `device_approval_before_password_change_is_rejected`, `device_approval_from_revoked_session_is_rejected`, and stale TOTP continuation. Assert rejection plus no newly active refresh session/enrollment access, fresh login succeeds.
- [x] Run focused tests before implementation and record the expected unauthorized-session bug.
- [x] Implement generation/snapshot binding and atomic final validation; keep existing same-flow retry behavior.
- [x] Run `cargo test -p cloud-server` against the disposable DB, then format and focused Clippy using pinned Rust. Commit and report with full RED/GREEN commands/results.

### Task 2: Repair sharing retries and operation accounting/concurrency

**Files:** features/invites/repositories.rs and integration_tests.rs; operations/repositories.rs and integration_tests.rs; new `202610150002_release_purged_operation_usage.sql`; tests/acceptance/specs/sharing.spec.ts; related tests only as required.
**Interfaces:** Consumes Task 1 unchanged auth wire model. Produces retry-safe invite response and consistent operation accounting; no new UI member-id flow.
**Requirements:** Resolve S01/S02/S03. Same normalized invite hash + same recipient is the exact redemption identity (no wire request-id addition needed); repeat returns the original package/role/epoch/current-state floor while entitlement/space/epoch remain valid. It must not increment use count or change membership again, allow a second recipient, revive removed membership or return obsolete epoch keys. Preserve an already-active member's entitlement on a retry. Add a new migration to release OLD.operation_bytes on cascaded parent deletion without double-counting direct op deletes and recompute existing account totals to repair drift. Acquire the strongest necessary space lock before quota/sequence work so distinct appends never upgrade competing SHARE locks. Preserve revocation and epoch rechecks and consistent lock order.

- [x] Add red tests for lost-response invite retry (and concurrent same/different recipient), expired unused invite, removed member/stale epoch, and acceptance same-recipient retry. Assert exact same ciphertext and metadata, unchanged used_count/membership.
- [x] Add DB tests for operation DELETE, space DELETE and workspace cascade, plus migration recalculation of pre-existing drift; assert owner totals equal live operations and another space's charge remains.
- [x] Add deterministic concurrent distinct-client-op append test; assert both accepted with distinct monotonic sequences and exact counters. Retain existing same-id retry tests.
- [x] Implement fixes, run DB-backed server suite and relevant sharing acceptance when available, format/check, commit and report RED/GREEN evidence.

### Task 3: Preserve DAV stream identity and enable isolated native HTTPS

**Files:** crypto-core-lib/src/local_bridge_runner/mod.rs, cache.rs and tests; tests/dav_conformance.rs; crypto-core-lib/Cargo.toml and desktop/src-tauri/Cargo.toml; focused transport test fixtures as needed.
**Interfaces:** Existing runner/public DAV API retained. Reqwest TLS capability must be explicit in independently built native crates.
**Requirements:** Resolve C01/C05. Existing resource writes use cached head stream_id and actual PIM kind, including Task tombstones and conflict copies; only new DAV hrefs derive a fresh deterministic stream id. Validate graph dependency before persisting invalid outbox state. Enable the supported TLS backend in both reqwest declarations rather than rely on cloud-server feature unification.

- [x] Add real HTTP DAV regressions editing/deleting first-party random-stream task/event/contact and conflict branches with ETags. Assert same-stream parent, immediate projection, valid tombstone kind, replay/retry semantics and no invalid queued operation after rejection.
- [x] Expand `signature_covers_every_client_field_and_ciphertext` to mutate each public envelope field and ciphertext separately, asserting verification failure; this strengthens an existing invariant and needs no artificial failing-before-code step.
- [x] Add an isolated feature/transport regression proving core FRB and desktop builds have TLS, with local synthetic CA HTTPS handshake using the real configured client or equivalent connector smoke. No production network required.
- [x] Run tests RED, implement minimal fixes, then core FRB tests, DAV suite and isolated native graph/build checks on pinned Rust. Commit with evidence and explain platform trust limits.

### Task 4: Repair desktop cloud decode, recovery packages and startup refresh recovery

**Files:** desktop/src-tauri/src/commands/auth.rs, collections.rs, common.rs and tests; shared core recovery/runner credential interfaces only where necessary.
**Interfaces:** Consumes Task 3 TLS and existing LocalBridgeRunner durable refresh state. Produces one credential-generation source shared by startup/active runtime; unchanged cloud wire schema.
**Requirements:** Resolve C02/C03/C04. All cloud response decodes use human-readable MessagePack semantics. Desktop collection recovery packages use HPKE EncryptedGroupKey addressed to the deterministic account recovery identity, matching FRB. Repair/rewrite a legacy malformed package only where the current authorized device holds the space key; never pretend unavailable old keys can be recovered. Before any startup refresh, consult cache replacement/pending-attempt state and use the same persisted rotation request id as runtime. A keychain-only fallback is valid only when no matching cache generation exists. Keep endpoint/account matching strict, preserve monotonic cursors and secure cache paths.

- [x] RED regression for nonempty ListSpacesResponse UUID strings/binary packages in runner and direct transport paths.
- [x] RED recovery wrapping round-trip with only account master/recovery identity on a second device, asserting schema and exact key.
- [x] RED startup credential-manager tests at three boundaries: server rotated/response lost; cache replacement committed/keychain stale; keychain current. Assert same request identity or recovered replacement and no all-session reuse path. Use fake keychain/network boundaries while exercising production selection/orchestration.
- [x] Implement, run desktop Rust suite and affected core FRB suite, format/Clippy, commit and report.

### Task 5: Preserve PIM temporal edits and recurrence across zones

**Files:** web/src/lib/pim.ts, components/app/TaskWorkspace.svelte, pim.test.mjs and focused editor helper/tests if needed; mobile/lib/src/models/bridge_models.dart, test/pim_models_test.dart.
**Interfaces:** Retain typed date/utc/zoned records and existing PIM operation wire schema.
**Requirements:** Resolve U01/U02/U03. Unchanged task due input preserves original record exactly (date kind, timezone, seconds), matching existing CalendarWorkspace behavior. An explicit due edit retains original zone unless user explicitly chooses another. For web recurrence, expand floating zoned local fields and map every occurrence through the original typed TZID before actual-instant comparison; pinned RRule 2.8.1 host-dependent rezoning must not affect results. Preserve COUNT and inclusive UTC UNTIL. For mobile UTC recurrence, anchor from UTC fields without device-local conversion; date and zoned cases retain their semantics. Preserve DST rather than deleting TZID.

- [x] RED task-title edit due roundtrips: Asia/Tbilisi 18:30/14:30Z in UTC browser, date-only, nonzero seconds; explicit time edit has expected zone/instant.
- [x] RED web weekly Asia/Tbilisi 2026-08-28 07:00–07:30: Sep 3 absent, Sep 4 present under TZ=UTC; include negative offset midnight, both host-offset signs, DST weekly fixture, COUNT, inclusive UTC UNTIL and all-day.
- [x] RED mobile UTC weekly 20260828T180000Z under TZ=Asia/Tbilisi: Sep 4 present, Sep 5 absent; also UTC, a negative offset, zoned DST and all-day.
- [x] Implement and run web check/unit/build plus Flutter analyze/test, including non-UTC subprocess/test environment. Commit and report.

### Task 6: Restore offline sessions and reconcile background projections safely

**Files:** web/lib/api/cloud.ts and auth/session-flow.js plus focused tests; mobile bridge_controller.dart/tests; services/background_sync_service.dart, system_projection_service.dart/interfaces/tests; ios/Runner/Info.plist, AppDelegate.swift and pinned-plugin setup as needed; narrow shared Rust FRB session guard and committed generated bindings where required for cross-engine coordination.
**Interfaces:** Uses existing secure runtime/vault/token/projection stores and real FRB release factory; scheduling id `kamori_background_sync_task`.
**Requirements:** Resolve U04–U07. Coordinate refresh across tabs and same-tab callers, bootstrap current CSRF at the correct locked boundary and preserve durable generation-specific request ids/lost-response semantics. Without Web Locks fail before network with a non-401 coordination error preserving local auth. Genuine revocation 401 still clears auth. Pending old-session revocation failure must not stop a distinct matching current snapshot from restoring offline; keep revocation queued. Extract a testable production background runner with injected OS/storage/network boundaries. After encrypted sync, persist rotated credentials safely and reconcile current opted-in projections under the matching account. No new permission prompt, no disabled collection projection, and provider failure must not undo successful encrypted sync. Serialize all foreground shared-runtime/session/token/opt-in mutations and complete background work with an existing-core process-global Rust/FRB opaque guard; retain native ownership through in-flight operations and commit generated bindings. Logout/disable may wait for an existing operation. Further unprotected snapshot rereads cannot replace this coordination. Configure iOS permitted task identifiers/background mode and native plugin callback according to installed pinned Workmanager; document physical-device gate accurately.

- [x] RED shared-cookie/tab CSRF rotation scenario, concurrent refresh callers, partial response/lost response and genuine revoked session.
- [x] RED controller restore with current vault/snapshot and queued other-session revocation throwing; assert local PIM available and queue retained.
- [x] RED background enabled create/update/delete projection, disabled skip, account mismatch and permission-free flow; assert token persistence survives independent provider failure.
- [x] Add configuration consistency check for Dart task id ↔ plist/native registration; do not claim physical scheduler delivery from this check.
- [x] Implement, run web check/tests/build and Flutter analyze/tests; verify iOS configuration syntactically and native compile if available. Commit and report exact limitations.

### Task 7: Preserve sourced secrets and terminate failed backup jobs correctly

**Files:** infra/main.go or shell environment helper, infra/cloud_init.go, infra/secrets_test.go; deploy/postgres/run-pgbackrest-backup, deploy/backup/replicate-blobs; focused Go shell-orchestration tests.
**Interfaces:** Separate existing Compose dotenv representation from POSIX-shell source representation. No production stack config changes or remote execution.
**Requirements:** Resolve I01/I02. Use shell-safe literal assignment encoding for sourced postgres/backup envs, round-tripping arbitrary supported secret characters without evaluation. Compose dotenv must also preserve literal dollar/quotes/backslashes: actual Compose parser reproduced interpolation of `VALUE="literal-$KAMORI_AUDIT_SYNTHETIC"` to `literal-expanded`. Fix its encoder too; verify via actual `docker compose config` (no daemon needed) against synthetic values, while shell-sourced files use separate POSIX encoding. Backup/replication failures after running must write a failed terminal heartbeat and preserve nonzero exit status, including archive check/inventory/hash stages; success emits one ok and no failed heartbeat. No production-only test hooks; use temporary fixtures to substitute absolute library/lock paths for shell tests.

- [ ] RED bash round-trip of each sourced environment with dollar, quote, backtick, command-substitution marker, tab/newline/backslash. Assert exact synthetic values and no marker execution; use actual Compose configuration parsing to assert literal values in both `env_file` and `--env-file` substitution paths.
- [ ] RED real-script tests using fake external commands for successful job, failed backup/check, failed replication copy/inventory/hash. Assert ordered running→ok/failed, and exit code preservation.
- [ ] Implement and run go test/coverage, go vet, gofmt check, bash -n for affected assets and CI detector tests. Commit and report.
