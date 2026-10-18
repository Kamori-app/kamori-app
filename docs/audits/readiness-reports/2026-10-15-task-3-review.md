# Task 3 review

## Spec compliance

- ✅ Spec compliant for `ba1c601173e355b1c0c23b05c4ad36c566e19872..a3c2efef62a7b11d1c61fa9d1687f213ae765bb3`. Both required two-order scenarios, durable assertions, current-generation controls, refresh exact retry and passkey one-use controls are present. References below use `apps/cloud-server/src/features/auth/handlers/persistence_race_tests.rs` unless otherwise stated.
- ✅ Refresh/reset: line 131 exercises the HTTP refresh route against the production reset repository transaction. Lines 162 and 177 observe the competing PostgreSQL backend before releasing the blocker. Line 197 checks replacement access revocation, the following assertions check rotation linkage and replacement refresh rejection, and lines 221–245 check generation, original access revocation, failed stale exact retry, zero live sessions, and a successful current-generation exact retry.
- ✅ Deferred registration/reset: line 362 prepares valid registration before racing the actual finish route. Line 397 observes the production commit transaction; line 428 requires no passkey after reset-first, while line 431 requires the committed passkey to survive finish-first. Lines 440–464 require revoked old authorization and consumed enrollment/registration state. Lines 466–511 cover fresh-session rejection of stale completion and valid new registration with one-use completion. Line 512 performs signed discoverable login followed by real device enrollment and authorization.
- ✅ Unrelated-account state is compared using complete user/session/passkey snapshots (line 70), after each order in both tests. The refresh test also checks the unrelated access capability remains usable (line 247).
- ✅ Scope is test-only: sibling inclusion at `apps/cloud-server/src/features/auth/handlers/integration_tests.rs:2021`, new test module, dev dependency at `apps/cloud-server/Cargo.toml:57`, and one dependency edge at `Cargo.lock:940`. No production coordination or transport change is in this diff. Reporting this as newly proven coverage, rather than a product RED/fix, is appropriate.
- ⚠️ The inherited shared setup and Task 2 database-prerequisite implementation are outside the supplied diff. This review verifies use of the existing setup, not its implementation. The preservation of all thirty earlier fixes is a broader branch-review responsibility; this task does not modify them. No independent branch/ref inspection was performed under the review's no-git constraint.

## Strengths

- Line 33 observes `pg_blocking_pids`, exact blocker PID, lock wait state and the expected SQL fragment. Sleep is observation polling rather than the ordering proof. In commit-first schedules, reset is shown waiting on the actual operation backend before the session blocker is released.
- The software authenticator at line 259 creates an ES256 key; line 286 constructs a bounded, documented none-attestation fixture; line 314 signs authenticator data and the client-data hash. Successful production registration plus signed login establishes useful validity beyond merely constructing plausible bytes.
- Assertions combine HTTP outcomes, persisted session/passkey state, durable access authorization and subsequent real authentication. The tests do not replace the final binding validator with a mock. Helpers keep the two substantial scenarios readable without changing the pre-existing integration module's structure.

## Issues

- Critical: none.
- Important: none.
- Minor: none requiring a code change.

## Named focused checks and limits

- **Risk: the observed passkey wait could be an early authorization transaction rather than the final persistence transaction.** Checked unchanged `services/passkeys.rs:80` through its finish implementation, `repositories/authentication.rs:14`, and `features/common/authz.rs:24`. Preliminary authorization uses an ordinary durable-session read. The final `binding.lock` at `services/passkeys.rs:111` runs in the same transaction as the passkey upsert and commit at line 121; it locks account then session. This establishes the intended wait placement.
- **Risk: the reset helper or refresh wait could omit production revocation or fail to establish account-before-session ordering.** Checked unchanged `repositories/refresh_tokens.rs` rotation lookup/transaction and reset implementation, particularly lines 197, 246, 257, 263, 387 and 478. Reset increments generation and revokes account sessions in the caller's transaction; refresh holds the account lock through replacement persistence. The staged reset therefore invokes the real persistence boundary. HTTP password/recovery admission is outside these new schedules and was not re-reviewed.
- **Risk: passkey state-absence assertions could use keys unrelated to production registration state.** Checked unchanged `platform/security/passkey.rs:136` and its registration key function at line 412, together with the already-inspected enrollment key in `services/passkeys.rs:89`. Both asserted keys match production one-use storage, and registration invokes the real WebAuthn verifier.
- **Evidence checked without rerunning suites:** `/private/tmp/kamori-readiness-20261015/auth-races-focused-2.log:21` records 2 passed; `auth-races-full-server.log:178` records 158 passed. Their lock diagnostics contain all six intended wait edges. `auth-races-clippy.log:2` records successful completion; no `warning:` or `error:` matches appeared in the focused/full/Clippy/fmt log scan. The fmt log has no matching diagnostics. Exit-status claims for empty-output checks remain the implementer's reported evidence.
- Read the saved full diff and recovered the initially truncated leading portion from that same saved package. No changed source file was opened separately. A line-reference extraction subsequently used only the saved package. One initial scoped search included a nonexistent `handlers.rs` path; the useful matches resolved the actual service/repository paths, and no source changes resulted.
- No new test run was warranted: source inspection and existing exact-source evidence answered the concrete doubts. These tests prove the combined generation/session contract under the two schedules; they do not independently mutation-test every redundant guard. Hardware authenticator UI, additional algorithms and external challenge-store lifecycle remain outside this task.

## Assessment

**Task quality: Approved.** The schedules establish actual conflicting production transactions and verify resulting authorization and persistence behavior, including valid recovery controls. No implementation change is needed before proceeding to the next task.
