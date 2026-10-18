# Final whole-branch review — Kamori audit remediation

Review target: `81248927f1ad734c246f582486d9840da6b47637..1be670b`, 20 commits. Worktree: `/Users/yujin/.codex/worktrees/d868/kamori-app`. Reviewer: `/root/review_final_audit`, 2026-10-15. **Five confirmed defects require fixes.** This is the single final whole-branch review, not a rerun of the task reviews.

## Scope and evidence

Read AGENTS.md, SPEC.md, the eight-task remediation plan, audit/report matrix and decisions, and the SDD ledger. Reviewed the supplied whole-branch diff in bounded passes; checked generated bindings structurally and metrics by metadata/aggregate validation. Read unchanged neighboring code only to resolve named authentication, refresh-lifecycle, lock-order and lease-ownership questions. The production checkout, index and HEAD were not modified. No subagents, production actions, secret/private-PKI reads, or full-suite reruns occurred.

The controller's existing evidence remains valid for what it covers: pinned Rust 1.97.1 with actual PostgreSQL (142 server + 69 FRB core + 14 DAV + 18 desktop tests), workspace formatting/strict FRB Clippy; Bun 1.3.14 web 66/admin 4 plus checks/builds; Flutter 68/analyze; real FRB Android release packaging for three ABIs; Go 1.25.11 full/vet and 15 shell cases; nine browser acceptance cases on fresh DB/rebuilt images; and a real Docker COPY exclusion check. I did not rerun these suites or treat their success as proof of the missing interleavings below.

Focused review evidence is under `/private/tmp/kamori-audit-20261015/final-probes/`:

- `mobile-lineage2.log`: actual native mobile import/configure/provision calls and a real SQLCipher cache, with a synthetic loopback HTTP server, reproduce F1.
- `mobile-cold.log`: the actual native configure/import/register/sync order reproduces F2.
- `locks.log`: two transaction schedules using the production tables/triggers reproduce F3 and F4 with PostgreSQL `deadlock detected`.
- `deletion.log`: actual multi-row DELETE plus the production new trigger and append's locking statements reproduce F5.
- `mobile/core/src/bin/review_mobile_probe.rs`, `lock_probe.py`, and `deletion_probe.py` retain the reproducers. The native probe uses a temporary source copy, changing only the `frb_api` module's visibility to call the existing APIs; it was built with pinned Rust 1.97.1 against the retained dependency target. The DB probes exercise the exact relevant SQL lock phases, not complete HTTP requests. Every probe transaction was rolled back and synthetic DB fixtures were removed.

## Strengths

- Authentication generation is carried through deferred OPAQUE/TOTP, approval, enrollment, reauth and passkey paths and rechecked in the committing transaction. Recovery-attempt invalidation preserves already-completed exact retries.
- Invite retries retain the original committed response without restoring removed membership or bypassing epoch/revocation checks. Single-space append serialization removes the original SHARE-to-UPDATE upgrade race while retaining role/signature/epoch checks.
- DAV identity preservation and pre-persistence reconciliation validation have meaningful real-HTTP regressions. Desktop refresh startup has an explicit trusted anchor, durable attempt identity, fail-closed old-lineage handling and a fresh-signin reset preserving the PIM cache.
- The mobile opaque lease is created synchronously before acquisition, retains native ownership during in-flight futures and covers foreground mutations as well as background jobs. The generated Dart/Rust content hash, all 28 dispatch names/IDs and serialized argument orders match. The guard fixes concurrency; F1/F2 concern credential lifecycle independently of that guarantee.
- Coverage metadata identifies toolchains/revisions, exclusions and changing populations. All eight normalized files have unique relative paths and consistent aggregate line/statement totals. Rust branch coverage is not misrepresented; web's expanded denominator and Flutter/helper-only scope are disclosed. Shell/Compose tests exercise their real consumers rather than merely checking renderer strings.

## Issues

### Critical

None classified Critical. The account-wide revocation in F1 and failures of security workflows below are nevertheless required fixes under the user's request.

### Important

#### F1 — Mobile startup replays a refresh token older than the durable cache lineage (P1; controller ID U09)

**Locations:** `packages/crypto-core-lib/src/local_bridge_runner/cache.rs:330`, `:379`; `local_bridge_runner/mod.rs:186`; `frb_api/state.rs:53`; `frb_api/bridge.rs:51`; `frb_api/devices.rs:133`; `apps/dav-bridge-mobile/lib/src/state/bridge_controller.dart:129`, `:838`.

After secure storage retains `(T0,R0)`, native rotations can commit `(A1,T1)` and then `(A2,T2)` before Dart persists the new secure credential. A process crash leaves cache current T2 with immediate predecessor T1. On restart, import writes T0/R0 only to the process globals. Mobile's unattributed cache takes the early return in `bind_refresh_scope`; `recover_rotated_credentials(T0)` returns None and the runner falls back to T0. Configure does not reconcile the global token with the durable cache, and restore proceeds into direct FRB provisioning using the old access/refresh pair.

**Executed result:** the probe stores both rotations through the actual runner persistence API, drops/recreates the runtime, imports T0/R0, configures and provisions. It prints `runner restored from secure T0 = Some("T0")`, then captures `GET /spaces` followed by `POST /auth/refresh` with T0/R0. This is not merely an inferred missing test.

**Impact:** the real server's exact-retry path requires T0's immediate replacement T1 still to be active (`apps/cloud-server/src/features/auth/repositories/refresh_tokens.rs:267`). Once T1 has itself rotated, replaying T0 reaches `:300` and revokes **every refresh session for the account**, including unrelated clients. The loopback probe confirms the client request; account-wide impact follows directly from this server branch. The session mutex cannot protect against a process crash.

**Fix:** give mobile one durable, appropriately scoped credential/attempt lineage shared by direct FRB transport and the runner. Reconcile token, access token and the same persisted per-generation request ID before any network-capable startup work. For an unanchored old secure token, fail closed without submitting it and allow explicit fresh signin to reset credential state while preserving offline data. Simply accepting the newest cache token without account/endpoint anchoring is unsafe. The controller confirmed this fail-closed restore/explicit fresh-signin policy after receiving the findings. Also preserve the imported pending request ID: runner attempts currently call `begin_refresh_rotation` independently (`mod.rs:193`), whereas direct FRB uses the imported global ID (`frb_api/transport.rs:50`).

**Regression:** restart after two or more rotations before secure persistence, plus lost-response/pending-attempt and immediate-predecessor recovery cases, using actual native/cache paths. Assert no stale-token request, correct reuse of the exact persisted attempt ID, no unrelated-session revocation, and retained offline data/fresh-login functionality.

#### F2 — Cold background sync constructs a runner before importing its refresh token (P2; controller ID U10)

**Locations:** `apps/dav-bridge-mobile/lib/src/services/background_sync_service.dart:58`, `:70`; `packages/crypto-core-lib/src/frb_api/bridge.rs:31`, `:104`, `:117`; `frb_api/auth.rs:176`.

A newly launched headless process starts with no global refresh token. `runBackgroundSync` calls configure first, which immediately builds and caches a runner with no refresh credential. The subsequent import changes only the global token and does not rebuild/update that runner. Registering keys and sync reuse it. On an expired access token, the runner cannot refresh. `persist_runner_refresh_token` then copies its None back into the global token, undoing the import.

**Executed result:** native configure → import T0/R0 → register collection → sync emits only `GET /spaces/<id>/devices`; it returns `cloud space device directory fetch: unauthorized and refresh failed`, emits **no** refresh request, and changes the exported token from Some(T0) to None. This uses the exact background ordering, not a fake Dart bridge.

**Impact:** background encrypted sync and the newly added projection reconciliation stop working once access expires on cold process starts despite a valid secure refresh credential. Normal mock tests do not model the runner/global split.

**Fix:** hydrate/reconcile the secure credential before runner creation, or make configuration/import one native operation that installs a consistent pair and attempt ID. Ensure importing a changed credential cannot leave an already-created runner stale. Coordinate this with F1, including account switches and explicit logout.

**Regression:** a real native cold-background flow with expired access and a valid refresh token must send the expected refresh request, retry the protected request, commit the replacement securely and complete projection. Keep the existing foreground/background lease tests.

#### F3 — Rotation snapshots and owner recovery still invert device/account locks (P2; controller ID S08)

**Locations:** `apps/cloud-server/src/features/spaces/repositories.rs:814`, `:925`; `apps/cloud-server/migrations/202608230010_signup_oplog_limits_and_indexes.sql:55`; `apps/cloud-server/src/features/auth/repositories/account_recovery_attempts.rs:88`, `:170`.

The ordinary append fix establishes space → owner → device order, but `revoke_member_and_rotate` still takes a snapshot author's device `FOR SHARE` before its operation insertion triggers an UPDATE of the owner's accounting row. Recovery takes that same owner row `FOR NO KEY UPDATE` before revoking devices.

**Reproduced schedule:** rotation holds space and owner-device SHARE; recovery holds owner and waits to update that device; rotation inserts its snapshot and the production accounting trigger waits for owner. PostgreSQL reports `deadlock detected` while updating `devices` in `locks.log`.

**Impact:** concurrent recovery and collection key rotation cause an internal error/aborted security operation. A rollback prevents partially committed rotation or recovery, but neither caller transparently retries the transaction. The defect is in an adjacent existing snapshot path and remains within the requested whole-repository business audit.

**Fix:** establish compatible space/owner/device ordering for snapshot rotation as for append, or use a common serialization strategy without introducing user→space inversions. Preserve authoritative signer revalidation, exact rotation retry, package coverage, epoch and quota behavior.

**Regression:** stage the actual rotation/recovery transactions in both orders with an owner's signed snapshot. They must serialize to a valid success/authorization outcome, with no 40P01/HTTP 500 and correct counters/epoch/session invalidation.

#### F4 — Ownership acceptance acquires the recipient row before its quota lock (P2; controller ID S09)

**Locations:** `apps/cloud-server/src/features/ownership/repositories.rs:345`, `:356`; `apps/cloud-server/src/features/cas/repositories.rs:39`, `:77`, `:573`.

Accepting space X locks `space, recipient` in the initial SELECT, then requests advisory quota lock 731 for the recipient. Blob reservation in a different space Y already owned by that recipient requests advisory 731 first, then locks the recipient's user/account counter row.

**Reproduced schedule:** blob holds Y and quota731(recipient); transfer holds X and recipient; blob waits recipient and transfer waits quota731. PostgreSQL reports the user-row/advisory deadlock in `locks.log`.

**Impact:** ordinary attachment upload concurrent with accepting a shared collection can fail with an internal error. Different spaces do not avoid the account-level cycle. This adjacent pre-existing issue was found while checking the explicitly named append/transfer/blob interaction.

**Fix:** make quota/account lock acquisition consistent across transfer and blob reservation. Preserve recipient authorization and acquire both account rows in a deterministic order; locking the recipient early defeats the later ordered account SELECT. Consider reciprocal transfers and FK-compatible lock modes when choosing the final order.

**Regression:** real transactions for recipient-owned-space blob reservation versus acceptance into the same account must complete without deadlock and retain accurate old/new owner usage and quota checks.

#### F5 — Multi-space account deletion violates the new space-before-owner order (P2; controller ID S10)

**Locations:** `apps/cloud-server/migrations/202610150002_release_purged_operation_usage.sql:15`; `apps/cloud-server/src/features/users/repositories.rs:237`, `:298`; `apps/cloud-server/src/features/operations/repositories.rs:86`, `:103`.

The new per-space BEFORE DELETE trigger correctly releases operation usage, but it takes the owner row while a bulk DELETE may still have more spaces to lock. Deletion of private spaces A/B can delete A and retain owner O, then wait for B. An appender on B already holds B and waits for O. Deferring `binding.lock` until after deletion does not prevent the earlier trigger from taking O.

**Executed result:** the probe creates two private spaces for one account, holds B with append's space lock, starts the same bulk DELETE predicate used by `delete_user`, verifies deletion already owns O using `NOWAIT`, and executes append's owner lock. PostgreSQL reports `deadlock detected` while locking `security_spaces` in `deletion.log`. No operations need exist: even subtracting zero obtains the owner row lock.

**Impact:** deleting an account with multiple private collections while its other device syncs can fail with an internal error after consuming reauth, or abort that sync. This is a composition regression introduced by the new accounting trigger.

**Fix:** ensure the full deletion set's space locks are acquired in a consistent order before any owner-accounting trigger runs, or provide a transaction-level strategy that safely retries the whole operation without losing one-use authorization. Account/workspace cascades need the same discipline; moving the user lock to the beginning recreates the inverse order with append.

**Regression:** deterministic multi-space deletion versus append, not only a one-space deletion/counter test. Assert eventual deletion/appropriate append denial, no 40P01/500, correct counters and unchanged deletion authorization checks.

### Minor

**M1 — Deferred Task 1 coverage gap remains, but is not an additional confirmed functional defect.** `apps/cloud-server/src/features/auth/handlers/integration_tests.rs:1735` exercises login issuance waiting for reset; there is no equivalent direct reset/refresh or deferred passkey-registration commit interleaving regression. I inspected the actual final bindings: refresh acquires the account before its refresh-session row, and passkey completion revalidates its retained generation/session binding in the persistence transaction. I found no stale-issuance path in those two mechanisms. Keep the gap in the business-tests matrix; targeted barrier-based regressions are valuable when that code is changed. It does not independently block this branch beyond F1–F5.

## Assessment of other named risks and reporting

The new single-space append locks the owning account, not the editor's account; membership and signer device remain authoritatively checked under locks. Same-space transfer/delete/rotation serializes on the space row. I found no separate shared-editor append/recovery bypass; the confirmed cross-path failures are F3–F5. Migration 002 obtains its EXCLUSIVE table exclusion before DDL/repair and recomputes zero as well as nonzero totals; retained original migrations and fail-closed legacy invite metadata are coherent. This does not negate F5's runtime batch-lock issue.

The FRB future owns an opaque clone until completion and explicitly releases it afterward; the synchronous empty handle avoids delivering a newly acquired orphan handle to a closed engine. Foreground state/credential mutations use the same lease. Third-party provider operations on engine destruction have the expressly documented weaker boundary below.

Metric snapshots correctly identify `dab0ba8` for final Rust/web/Flutter and `35a4b15` for Go, rather than pretending every snapshot is current HEAD. No separate metric defect found. The main audit/journal must incorporate all five findings before claiming all confirmed issues fixed. Refresh relevant metrics or retain explicit revision provenance after fixes; do not silently relabel old snapshots. Controller-only Markdown table/I01 wording edits made during review do not alter the production verdict.

## Declined to judge

These are explicit set-asides for controller adjudication, not assertions that the behavior is unimportant or unsupported:

- **Task 1 direct reset/refresh and passkey-registration interleaving evidence:** no dedicated regression; judged as M1 coverage work after inspecting binding code, not a demonstrated additional defect.
- **Physical iOS compile/link and release execution:** the SDK query returned exit 69 for an unaccepted Xcode license. Syntax/plist checks do not prove compilation. No terms acceptance, sudo or SDK bypass was attempted; this remains an external release gate.
- **Physical Android/iOS Calendar/Contacts behavior, scheduler delivery, permissions and lifecycle:** mock/platform-channel tests and Android packaging do not prove actual providers or delivery after suspension/restart. The pending iOS request check claims admission only. No device was available for that validation.
- **OS provider calls surviving abnormal Flutter-engine teardown:** native FRB futures retain the lease, but third-party platform calls do not. They may finish after teardown. The accepted SPEC §9 interrupted-projection/reconciliation boundary is accurately documented; atomic provider rollback or remote wipe is not established.
- **Real OS keychain/Keystore failures and physical power loss:** desktop regressions use a credential-store boundary and the mobile probes use real SQLCipher with synthetic crash states. They do not certify hardware storage or every OS failure mode. F1 is a concrete source/runtime failure within that boundary and is not deferred.
- **Desktop secure token older than the immediate predecessor:** accepted fail-closed fresh-signin behavior preserves PIM data; no automatic recovery from an unprovable credential anchor is promised.
- **Android private-CA trust:** the accepted Mozilla-root policy intentionally excludes private Android CAs; desktop/iOS platform trust and Android release packaging are not a complete physical cross-platform TLS matrix.
- **Browsers without Web Locks:** the accepted behavior rejects automatic refresh locally without discarding the authenticated/offline session. A wider browser-support promise was not supplied, and no unsafe fallback was reintroduced.
- **Legacy consumed invites without recorded response metadata:** intentionally fail closed after migration rather than reconstruct potentially changed entitlements. Requires a new invite; the documented decision is consistent with the data available.
- **Imported recurrence exceptions (EXDATE/RDATE/RECURRENCE-ID) in first-party calendar rendering:** baseline audit records lossless preservation but incomplete rendering/end-to-end fixtures. The full unchanged materialization/display path was not retraced in this bounded final review, so no new defect or compatibility guarantee is asserted.
- **Mobile nonexistent DST wall-time normalization:** the baseline report's spring-gap example remains an unconfirmed concern without tracing UI validation through persistence. The reviewed U01–U03 fixes do not establish a general invalid-time policy.
- **Other outbox/materialization/cursor/epoch crash boundaries and imported DAV/PIM corpus interoperability:** meaningful new identity/temporal tests do not constitute exhaustive process-kill or external-client certification. Existing SPEC release gates and the business matrix retain these gaps.
- **Full user/admin hardware WebAuthn ceremonies and operator lifecycle:** server binding and wire tests are useful but do not prove physical authenticator flows, every admin mutation or every browser ceremony; no such runtime was used here.
- **Remaining server matrix gaps (workspace-transfer edge cases, distributed limiter/store outages, streamed egress crash metering and all boundary combinations):** recorded gaps alone are not confirmed defects; only the named concrete adjacent lock risks were extended into unchanged code here.
- **Production infrastructure/bootstrap, live deployment, PITR/DR and external security/legal review:** shell/Compose/Docker-context regressions are local evidence, not operational certification; these require the separately documented external gates and were not authorized in this review.

## Verdict

**Ready to merge? No — fix F1–F5.** The branch substantially improves the originally identified behavior and has broad successful verification, but real native probes expose two mobile credential failures and real PostgreSQL schedules expose three surviving lock cycles. Existing tests do not cover these compositions. One focused fix wave with regressions for these five issues is required before claiming the user's “fix every confirmed defect” objective complete.
