# Final whole-continuation review

Reviewed on 2026-10-15 as the single final Senior Code Reviewer. **Current result: 1 Critical, 1 Important, 1 Minor; not ready to merge.** This is a review of the entire continuation, not another task-scoped review or approval of a future fix.

- Baseline: `dc13f1909cc9acff3e13d8fe849a26943efe17ce`.
- Reviewed HEAD: `cdca2304efda494c5b2167736ab5a49221997201`, branch `fix/audit-remediation`, existing checkout.
- Supplied package: `.superpowers/sdd/2026-10-15-release-readiness/review-dc13f19..cdca230.diff`, 23 commits, 126 files, 908,263 bytes / 15,052 lines. Read in bounded passes, including changed application source, tests, build/CI configuration, restore tooling and generated Rust/Dart bindings. Used the supplied package rather than deriving another whole diff.
- Requirements: original audit/business-test/coverage objective and preservation of its thirty fixes; accepted eight-task plan and Global Constraints; user/root AGENTS product constraints; SPEC account, transport, offline, PIM, DAV, mobile and release contracts; main readiness audit, decisions, business-test matrix, final-gates report, handoff and relevant permanent task reports/reviews.
- No subagents, checkout/ref/index/source changes, suite reruns or shared-infrastructure resets. The only executable reproduction was the named in-flight browser-isolation doubt, using an ephemeral loopback peer and private synthetic browser state. This report is the sole repository write. Final read-only check found a clean tracked tree, unchanged reviewed HEAD/branch, and protected ref `codex/pause-hosted-infrastructure` still at `81248927f1ad734c246f582486d9840da6b47637`.

## Strengths

- The continuation adds evidence at consequential boundaries. Missing-DB tests exercise the actual filtered test executable without either URL, and Turbo no longer caches that database-dependent task. Reset/refresh and reset/passkey tests stage both transaction orders with real PostgreSQL blocking and final persisted-state assertions; the passkey flow uses an actual software authenticator response, not only a mocked binding validator.
- The invite/delete correction takes account gate 913 before space/invite locks and rechecks eligibility. The old ownership scenario was restaged through real invite admission and production acceptance/deletion paths rather than bypassing the new guard. Contention measurements retain the synchronization they measure, exact outcomes and bounded local scope.
- Native integration calls the real FRB and installed Calendar/Contacts/secure-storage/background channels. The missing-contact correction distinguishes an absent record from a provider error. Debug configuration is now a tracked build input, and the scoped xcrun wrapper avoids global Xcode changes. Generated FRB field order/types agree across Rust and Dart; final native evidence checks the actual linked framework and packaged ELF ABIs.
- Imported-calendar work preserves raw iCalendar authority, unmanaged properties/alarms and detached components through partial edits. The shared corpus, real DAV round-trip, stable imported href/logical resource identity, exact temporal duration, moved/cancelled occurrence identity and series-level UI actions are substantive checks. Unsupported native exception-set projection fails explicitly while independent supported collections still reconcile; failed cleanup retains its mapping and original error.
- Desktop/admin gates mount production components and assert boundary payloads, pending/error recovery and privileged ceremony ordering. CI provisions the pinned browser and selects the frontend jobs for external UI/corpus inputs; Turbo inputs include those files. The admin confirmation correction is small and protects the visible confirmation requirement.
- Restore evidence uses actual SQLx migrations, PostgreSQL 16 physical encrypted PITR into separate PGDATA, migration/schema/accounting/ciphertext comparisons and page checksums. Blob verification reads the independent DR store after stopping the primary and detects missing and same-size-corrupt objects. The runner's fixed Podman endpoint and attempted-container cleanup fixes have actual daemon controls, ownership refusal and independent inventories.
- Reports distinguish newly demonstrated defects from already-green coverage, retain failed harness attempts, avoid adding overlapping Rust counts, and separate local/simulator/unsigned results from public-release certification. Historical coverage is revision-qualified rather than represented as a fresh percentage.

## Issues

### Critical (Must Fix)

### C1 / R16 — An in-flight Alice sync overwrites Bob's encrypted materialization after account switch

**Files:** `apps/web-frontend/src/lib/components/app/AppWorkspace.svelte:193`, `:217`, `:888`, `:907`, `:912`, `:1452`, `:1565`; `apps/web-frontend/src/lib/cryptoVault.ts:768`–`:775` and `:651`–`:663`.

**Trigger and mechanism:** Start manual sync for Alice with a queued edit; let its real append request reach the peer and hold the response. Unmount/sign out of Alice, lock the vault, then unlock and mount Bob with a different account key before releasing Alice's successful append acknowledgement. `runDataPlaneExclusive` validates the captured workspace only before calling `operation()`. Once that callback has entered, `flushOutbox` resumes after the network await, updates Alice's component-local operation states and calls `storeMaterializedPimState` without validating the lifetime. That function obtains the currently active key and scope, so it encrypts Alice's state for Bob and overwrites Bob's durable materialization. Queue removal likewise uses the later mutable scope. The common data-plane Web Lock serializes callbacks; it does not prevent account/vault activation while an admitted callback is suspended.

This is a reachable product lifecycle, not a conclusion drawn from an absent cancellation framework. The unchanged `routes/app/+layout.svelte:172` logout path clears the token, locks the vault and sets accessToken null without draining manual sync; its logout buttons remain enabled (`:305`, `:374`). Its auth conditional (`:243`) unmounts the workspace. `SignInModal.svelte:227` unlocks the new user's vault and then publishes the new identity. `AppWorkspace.svelte:2122` stops future automatic scheduling/registration but does not stop the admitted manual callback. These unchanged paths were inspected specifically to establish reachability.

**Actual RED on the reviewed HEAD:** `/private/tmp/kamori-readiness-20261015/review-inflight-probe.mjs` builds the existing browserDurability fixture with Bun 1.3.14 and mounts the production AppWorkspace in Chromium, with real WASM, encryption, IndexedDB and Web Locks. Only its HTTP peer/authentication boundary is synthetic. It creates a nonempty Bob vault using key byte 8 and Alice using key byte 7, reloads before manual sync to remove earlier automatic work, holds the received Alice append, switches/unmounts, asserts Bob's exact preexisting durable snapshot while held, then releases and waits until data-plane locks drain. Later network requests return a synthetic offline error; the held acknowledgement still succeeds. The assertion exits 1.

| Durable observation | Before held response is released | After release |
|---|---|---|
| Bob materialized item | `Bob durable sentinel`, space `00000000-0000-0000-0000-000000000042`, operation `d05a0f76-13ba-4372-bd8d-3a515421731f`, sequence 0 | `Alice private task`, space `00000000-0000-0000-0000-000000000041`, operation `a5a6bb43-3cf2-4b3b-8913-68bbb0070522`, sequence 1 |
| Bob outbox | Bob operation above | Exactly unchanged |
| Alice materialization after reopening Alice | Alice operation, sequence 0 | Exactly unchanged |
| Alice outbox | Alice operation above | Exactly unchanged |
| Cursors | `{}` for both accounts | Unchanged |

Full snapshots, requests and schedule: `/private/tmp/kamori-readiness-20261015/review-inflight-evidence.json`. Assertion/output: `/private/tmp/kamori-readiness-20261015/review-inflight-probe.log`. Bob's snapshot immediately after switching and before release equals his original snapshot. Therefore the late acknowledgement, not account setup, causes the cross-account write. The initial sandbox EPERM on loopback bind was a separate harness failure; the authorized local execution produced this actual RED and closed its browser/server.

**Impact:** Violates SPEC offline/account isolation and Task 4's requirement that switching never expose another account's cache. Alice's plaintext becomes durably readable under Bob's key and replaces Bob's materialized view. Bob's outbox survives, so this evidence is not a claim that every copy of his original edit is irrecoverably lost; the confidentiality and cache-integrity violation is already Critical.

**Smallest sufficient fix:** Carry the captured workspace/vault lifetime through the admitted asynchronous operation and validate it after relevant awaits and before persistence or shared UI/auth side effects. Stop stale work through the surrounding catch/control flow; a swallowed stale error must not let later stages run under Bob. Scope/key-dependent vault operations must capture the intended scope/key and validate the lifetime across their own awaits, especially encryption and database opening, rather than choosing a later global scope. Preserve existing serialization and normal current-lifetime acknowledgement behavior. A per-session runtime rewrite or generic cancellation architecture is unnecessary.

**Covering test:** Add the held-response/unmount/switch/release schedule to the actual mounted browser suite with different keys and nonempty Bob state. Assert exact materialization, outbox and cursors for both accounts before/after and on reopen, including the held pre-switch request identity. Keep normal acknowledgement/idempotent retry, queued-switch, same-account lock/reopen and concurrent-tab controls. A test that merely queues behind the lock cannot cover this defect.

### Important (Should Fix)

### I1 — The deployed encrypted-restore smoke checks a 2030 event in a 2026 Agenda window

**File:** `tests/acceptance/specs/smoke.spec.ts:61`–`:62`, `:94`–`:98`.

The test creates `Acceptance event` on 2030-01-02, reloads and signs in, then selects Agenda without navigating to that date. Task 6 correctly makes Agenda a seven-day occurrence view. The retained trace shows the fixed 2030 inputs and no date navigation before the assertion; the actual failure snapshot displays **Sep 28 – Oct 4, 2026**. The initial anchor is the first day of the displayed month (`CalendarWorkspace.svelte:18`), so simply changing the fixture to today's date without navigating is also insufficient when today lies outside that first week.

The fresh all-three-image full acceptance run on application-equivalent `0b1cc7f` exited 1 with **28 passes / 1 failure**, no retries. Failure is the title visibility assertion at line 98. Evidence: `/private/tmp/kamori-readiness-20261015/final-acceptance.log` and `acceptance-initial-artifacts/test-results/smoke--smoke-infrastructur-b6bf4-pted-offline-PIM-round-trip/{trace.zip,error-context.md,test-failed-1.png,video.webm}`. The later reviewed changes do not alter these application/test inputs.

**Impact:** The required repository acceptance gate is red, including this `@smoke` PR scenario. Its current failure does not establish that encrypted persistence lost the event; the UI is displaying a different date range. Leaving the failure would also obscure a genuine persistence regression.

**Smallest fix:** Use one captured browser-local fixture date and explicitly navigate the restored Agenda to the week containing it; a current-day fixture plus the Today control is suitable when the captured date remains in that week. Alternatively navigate explicitly to the existing fixed 2030 date. Preserve the deployed reload/sign-in and exact restored title assertion. Do not remove/skip the assertion, expand product Agenda to all time, freeze the whole suite's date, or increase retries/timeouts. If using a live current date, keep navigation tied to the captured date so a week-boundary crossing cannot invalidate the fixture.

**Covering verification:** Run this affected deployed smoke against the freshly built affected app, require the saved event to be visible after restore in the selected week, then perform the planned final full acceptance run with no concealed retry. Existing mounted Agenda tests should retain window exclusion and recurrence/override assertions.

### Minor (Nice to Have)

### M1 — Reverse-order invite/delete evidence prints an aggregate it did not measure

**File:** `apps/cloud-server/src/features/operations/readiness_contention.rs:311`–`:321`.

`readiness_invite_admitted_first_is_revoked_by_account_deletion` queries the single redeemed `(recipient, space)` membership and asserts `status == "revoked"`, then prints the literal `active_memberships=0`. This measures the particular membership's revocation, not the recipient-wide aggregate advertised by the marker. The other ordering's aggregate measurement does not establish this ordering's aggregate.

**Impact:** Overstates the precision of saved concurrency evidence. This is not evidence of a remaining production deletion race or a failed workload.

**Smallest fix and test:** Query the recipient's actual active-membership count, assert zero and interpolate the result; alternatively narrow this marker/report to the measured `member_status=revoked`. Run the affected named reverse-order test with the dedicated database and inspect its saved marker. No broad benchmark repeat is needed for a marker-only correction.

## Named candidates and deferred-minor rulings

| Candidate | Final judgment |
|---|---|
| In-flight manual network-await → persistence after switch/unmount | **Confirmed Critical C1/R16**, with the concrete production path and actual distinct-key durable snapshots above. Queued-work and hydration guards remain useful but do not cover this schedule. |
| Nine browser-durability cases omitted from PR smoke/default web tests | **No separate contract defect.** `tests/acceptance/package.json` intentionally defines smoke with `--grep @smoke` and full without a filter; `playwright.config.ts` includes `./specs`; `.github/workflows/ci.yml:354`–`:367` selects PR smoke versus non-PR full. All nine untagged durability cases actually ran in the parent full acceptance population. Task 4 requires genuine focused/full acceptance evidence, whereas Tasks 5/6 explicitly require the new frontend UI/corpus cases in PR gates. The detector's browserDurability fixture classification as acceptance/server input and web default's four recurrence cases match that split. These nine regressions are **not protected by the PR smoke subset**; that limit must remain explicit. Promoting a critical isolation case is a useful optional policy improvement, not a retroactive requirement that all acceptance scenarios be smoke. |
| Task 7 literal active-memberships marker | **Confirmed Minor M1**, as above. |
| Expected ERROR for configured quota exceeding database range | **No current defect.** `features/cas/services.rs:406`, `:421` deliberately exercises rejection of an unrepresentable quota. The successful negative test's ERROR output must not be represented as an internal error in the separately measured 300-transaction workload, or as warning-free suite output. Retain honest classification rather than suppress the production error to beautify tests. |
| XCTest SWIFT_DEBUG_INFORMATION, framework deployment target, AppIntents metadata and restoration-marker notices | **Nonblocking toolchain/test-host diagnostics on the tested configuration.** The recorded sequential XCTest/native runs pass; the lower deployment-target warning concerns RunnerTests linking the SDK's XCTest frameworks, not proof that Runner cannot run on a supported OS. No current runtime defect is established. Earlier OS/device execution remains an external coverage limit. |
| CocoaPods fallback / future SPM-only Flutter warning | **Future compatibility work, not a current pinned-build failure.** Current fallback is committed and the real framework builds/links. Track migration before a Flutter upgrade; do not change dependencies or the working packaging integration just to suppress this notice. |
| FRB INFO absent-FVM fallback, skipped MobileOpaqueSuite parsing, lifetime hint | **No demonstrated generated-binding defect.** These are disclosed informational generator diagnostics; reviewed paired generated structures and final real-FRB native executions/packaged symbols provide current boundary evidence. They do not certify every future generator/toolchain. |
| Playwright NO_COLOR/FORCE_COLOR conflict | **Addressed in scoped command environment.** Subsequent package UI commands use `env -u NO_COLOR`; no global environment change is needed. Historical warning remains correctly disclosed. |
| Empty Svelte adapter index-server/env chunks | **Nonblocking build output in the recorded successful builds.** No missing import, runtime failure or required application behavior is tied to these empty generated chunks. Removing empty output or adjusting bundler warnings would be optional build polish. |
| Unsigned iOS warning | **Expected for the explicitly requested no-codesign gate**, not a claim of a signed IPA. |
| Task 8 I1 endpoint binding | **Addressed by 25f0bec.** `run.py:22`–`:28`, `:59` uses the absolute Podman backend, fixed local `--url`, removes conflicting selectors and rejects unsupported engine overrides. The saved five-control run includes actual daemon endpoint cases. |
| Task 8 M1 partial container start | **Addressed by 25f0bec.** `run.py:103` records the attempted name before dispatch, cleanup inventories and checks ownership before removal, and verifies final absence. Actual post-allocation exit 127, pre-allocation exit 125 and label-refusal controls pass. Inspected `/private/tmp/kamori-readiness-20261015/restore-controls-final.log` and `restore/controls-xmbfp44h/final-owned-inventory.json`; five tests pass and owned inventories are empty. No full restore rerun is justified by this orchestration-only correction. |
| Earlier Task 6 minor actual-end / RDATE-only series-delete issues | **Addressed in source and mounted assertions.** Occurrence rows use actual start/end; the common series predicate includes RDATE for editing/deleting the original item. No remaining minor at those sites. |

## Verification and review boundaries

The reviewer executed only the named browser probe. Parent gates were inspected as existing, revision-qualified evidence, not claimed as reviewer reruns: Rust workspace **262 named entries**, separate overlapping FRB **97**, strict fmt/Clippy; three uncached frontend test/build tasks; Flutter **95** and analyze; actual iOS **6** provider integrations and **3** XCTest checks with real unsigned arm64 packaging; Android three actual packaged ELF ABIs. The final-gates report contains command, artifact/hash and source-input provenance. Native production inputs remain `341d976`; the later native change is the isolated `cfg(test)` credential timeout measurement. None of those passing gates overrides C1 or the actual full acceptance failure.

The full acceptance population has nine deployed-stack scenarios and twenty mounted-component scenarios with synthetic peers (nine durability, seven UI, four recurrence); do not call all 29 deployed end-to-end synchronization tests. There is no new line/branch coverage measurement. Test count growth, overlapping populations and a strong business matrix do not prove a current coverage percentage or complete coverage of all schedules; C1 demonstrates why.

Unchanged code was inspected for named risks only: real logout/sign-in/unmount reachability, vault write/delete scope selection, admitted sync call paths and access retry side effects; existing account gate and ownership/delete lock ordering; smoke date anchor and actual CI selection. Task reports were used for provenance, while findings and acceptance judgments were checked against code and saved runtime artifacts. The review does not retroactively certify every pre-baseline source line or replace the original thirty-fix reviews; no continuation change was found that requires undoing their transaction/lease/transport/product constraints.

## Recommendations

1. Fix C1, I1 and M1 in the one planned combined wave. Keep the critical fix bounded to operation lifetime and account-scoped persistence safety; preserve queued-switch, hydration, same-account-reopen, refresh-generation and idempotent acknowledgement behavior.
2. Verify the new held-response schedule first, run affected web/type/unit/build and focused database checks appropriate to the actual changes, then rerun final fresh affected-image full acceptance. Record commands, source inputs, exact durable observations and any intermediate failures. Do not re-execute unrelated native/restore suites when their source inputs are unchanged.
3. Keep release status honest until the complete gate passes. Historical 28/29 evidence should remain, followed by the corrected result; a green focused smoke alone does not replace the final full run.
4. Consider placing the critical account-isolation regression in the PR gate or documenting an explicit required full-acceptance pre-merge run. This is an optional strengthening of the present smoke/full policy, separately identified from the required bug fix.

## Declined to judge

Each line below identifies a considered behavior set aside and the reason; none is silently treated as proven.

- **Generic cancellation/per-session runtime architecture:** not required to fix the demonstrated isolation failure; judge C1 by exact side effects and lifetime safety. The accepted plan excludes a new runtime architecture, while still requiring the actual bug to be fixed.
- **Changing Calendar/Agenda's initial anchor to today:** no separate correctness defect established. Month/week/Agenda share the selected anchor, visibly label the range and expose Today; Task 6 deliberately retains that navigation contract. A preferred default-current-week UX is distinct from the smoke's obligation to select its fixture's week.
- **Making every durability acceptance case run on every PR:** the current explicit smoke/full split does not promise this. The delivery limit is disclosed above; optional promotion of critical cases remains an executor policy choice.
- **Physical iOS/Android provider behavior, background scheduler delivery, hardware secure-store/data protection and power-loss durability:** the available executions are simulator, process/tab-reopen and packaging evidence. Physical/scheduler/power-loss assertions need the corresponding devices/fault boundary and remain public-release gates.
- **Signed/notarized distribution, TestFlight/store submission and Android runtime execution:** unsigned iOS and three-ABI APK inspection do not establish these; credentials, signing and devices are outside this local authorized continuation.
- **Remote B2/Hetzner IAM, retention/TLS/network failure, production RPO/RTO and actual restored-application authentication/sync/export:** local POSIX pgBackRest and independent MinIO canaries establish storage/job correctness only. Canaries are intentionally not valid signed app messages. The runbook records the additional remote/application exercises.
- **Arbitrary SIGKILL/host failure during restore resource provisioning:** no such recovery guarantee was executed or certified. Current controls cover reported endpoint selection, failed/early container start and ownership refusal; general orchestration crash recovery would require a separate scoped fault exercise, not an inferred extra pass from those controls.
- **All external DAV clients, arbitrary imported recurrence forms and advanced exception authoring:** the real local DAV/shared-corpus boundary is exercised. PERIOD/RANGE expansion and native exception-set projection remain explicitly unsupported, raw data is preserved, and authoring is deferred; a broader client/format matrix remains a stated release exercise.
- **Remote Ubuntu/GitHub Actions execution and a second pristine-checkout native build:** workflow/input inspection and local installed/native execution cannot certify unexecuted environments. CI browser provisioning and tracked build inputs were reviewed; the reports do not claim these extra executions.
- **Future Flutter/SPM/generator compatibility or warning-free third-party tools:** current pinned functional gates pass, and each known notice is classified above. Future upgrades need their own compatibility work; no speculative dependency upgrade is requested here.
- **A new numerical line/branch coverage result or exhaustive re-audit of all thirty pre-baseline fixes:** none was measured/reperformed in this continuation review. Historical coverage retains its revision, the business matrix identifies proven behaviors and limits, and reviewed continuation changes must preserve the original accepted fixes.
- **Production deployment, legal/operator release approval and independent security-audit certification:** no such action or certification was authorized or performed; this is a repository code/readiness review only.

## Assessment

**Ready to merge? No.**

The continuation supplies substantial real-boundary evidence and preserves the main architecture, but an actual late manual-sync acknowledgement crosses the account confidentiality boundary, and the required deployed smoke remains red. Fix the three concrete findings, verify their affected behavior and complete the planned full acceptance gate before a readiness approval; the external public-release limits remain separate afterward.
