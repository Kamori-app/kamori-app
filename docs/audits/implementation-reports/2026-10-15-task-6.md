# Task 6 — offline sessions, coordinated refresh, background projection

Status: DONE_WITH_CONCERNS (iOS native build and physical scheduler/provider validation remain externally gated).

## Behavior and scope

- Web refresh now shares one promise among concurrent callers in a tab and uses an origin-scoped Web Lock across tabs where Web Locks are available. It forcibly bootstraps the current shared-cookie CSRF value **inside** the lock and sends that exact value with the existing durable generation-specific rotation request ID. A response failure retains the attempt: unchanged cookies reuse the ID; advanced cookies bootstrap a new generation. Genuine refresh 401 still reaches the existing session-expiration callback. Existing endpoint/MessagePack response models and IndexedDB attempt storage are unchanged.
- Mobile restore retries a queued previous-session revocation best-effort. Failure preserves the queue and warning while allowing a matching current runtime/vault to load its encrypted offline PIM. A second offline provisioning error retains both warnings. Existing account/vault checks remain strict.
- Extracted `runBackgroundSync` is the production callback's orchestration, with injected bridge, runtime/vault/cache-key/token stores, and projection service. It respects the persisted disabled preference, reconstructs the encrypted runtime, syncs, rereads the current snapshot, and skips obsolete completion after account switch/sign-out. The existing observed-token replacement guard remains. Rotated token plus request ID are persisted before listing/projecting PIM. It configures projection for the matching vault account and reads current collection opt-ins through `projectEnabled`. Projection/provider failures are logged independently and do not fail completed encrypted sync.
- iOS plist permits `kamori_background_sync_task` with `fetch`; native launch registers the pinned plugin callback and periodic identifier. The implicit-engine application registrar provides the admission-check channel. The pinned Apple plugin silently logs submission errors, so Dart checks for a pending request and rejects false/missing/error responses. Scheduling failure disables the persisted background preference with an existing UI error and does not roll back an authenticated login or offline restoration. Android scheduling skips this channel.
- No Rust/generated bindings, temporal code, registration policy, DAV behavior, fixed mobile cache path, projection opt-in semantics, or release bridge factory changed. No real OS stores, credentials, production services, deployment, push, Android rebuild, global settings, Xcode license, or app scaffolding were touched.

## RED → GREEN evidence

1. `bun test src/lib/auth/refresh-coordination.test.ts` (initial test filename; subsequently converted to the repository's `.test.mjs` convention): **1 pass / 4 fail**. Shared cookie rotation and concurrent refresh produced HTTP 401; lost-response/partial-response retries observed CSRF bootstrap outside the lock. Genuine revoked-session behavior already passed. After implementation, the same five regressions passed. Tests execute actual `cloudApi.refresh`, MessagePack transport, and session-flow; only fetch, Web Locks, and persistence are substituted. They model another tab advancing shared cookies; they are not a physical browser multi-tab test.
2. `flutter test test/bridge_controller_test.dart --plain-name 'queued other-session'`: RED `Expected true, Actual false` for authenticated state. GREEN verifies current PIM loads and queued old token remains. Strengthened the fixture so **both** revocation and current provisioning fail while persisted collections remain; this exposed warning replacement (`Expected contains revocation`, actual only reconciliation warning), then passed after preserving both warnings.
3. Extracted the existing callback without adding projection behavior, then ran `flutter test --no-pub test/bridge_controller_test.dart --plain-name 'background runner'`: **3 pass / 3 fail**. RED: projected OS contacts remained empty, provider path never ran, and a disabled scheduled job still synced. GREEN: production runner plus real `NativeSystemProjectionService` creates, updates and deletes opted-in contacts, excludes a disabled collection, rereads opt-ins after sync, makes no permission call, persists rotated credentials despite an OS failure, rejects mismatched vaults, skips disabled jobs, and leaves a switched account's token/provider untouched.
4. `flutter test --no-pub test/bridge_controller_test.dart --plain-name 'iOS admits'`: RED missing plist permitted identifier (`Actual null`), GREEN after plist/native registration.
5. Scheduling channel/preferences RED command: `flutter test --no-pub test/background_scheduling_test.dart test/bridge_controller_test.dart --name 'admission|iOS returns|iOS rejects|Android scheduling'`: **1 pass / 4 fail** (no admission check, false/error silently succeeded, failed preference remained enabled). Additional login regression RED: valid login became unauthenticated after scheduler failure. GREEN targeted suite: **35 tests passed**.
6. Initial web check found the new TypeScript test lacked Bun declarations and an inferred JS optional callback; converted the test to existing `.mjs` convention. Initial Flutter analyze found one missing brace in a test; fixed. Test-fixture corrections also included the pinned Contacts wire field `name.first` and a nonempty persisted offline collection. These setup issues are distinguished from the behavioral RED evidence above.

## Final verification

Executed from the worktree with Bun **1.3.14** at `/Users/yujin/.bun/install/cache/@oven/bun-darwin-aarch64@1.3.14@@@1/bin/bun` (its directory first on PATH) and Flutter **3.47.0** at `/opt/homebrew/share/flutter/bin/flutter`:

- `bun run --filter web-frontend check`: exit 0, **0 errors / 0 warnings**.
- `bun run --filter web-frontend test:unit`: exit 0, all affected web units passed, including all five new refresh coordination cases.
- `bun run --filter web-frontend build`: exit 0. Adapter emitted existing empty `index-server` / `env` chunks; production build completed.
- `flutter analyze --no-pub`: exit 0, **No issues found**. Log: `/private/tmp/kamori-audit-20261015/task6-flutter-analyze.log`.
- `flutter test --no-pub --coverage`: exit 0, **54 tests passed**. Log: `/private/tmp/kamori-audit-20261015/task6-flutter-tests.log`. The intentionally unavailable fake provider emits the expected caught projection-failure log; the test confirms sync success and persisted rotation.
- Copied `apps/dav-bridge-mobile/coverage/lcov.info` to `/private/tmp/kamori-audit-20261015/flutter-final.lcov`. Raw instrumented line counts: background service **51/76**, controller **411/504**, projection service **91/282**; aggregate **989/4406 (22.45%)**. This aggregate includes all instrumented files and is not a normalized non-generated coverage claim; controller owns normalization/comparison.
- `plutil -lint apps/dav-bridge-mobile/ios/Runner/Info.plist`: exit 0, **OK**.
- `/Library/Developer/CommandLineTools/usr/bin/swiftc -frontend -parse apps/dav-bridge-mobile/ios/Runner/AppDelegate.swift`: exit 0. **Syntax only**, no iOS SDK typecheck/link claim.
- `git diff --check`: exit 0.

## Native source and external gates

Inspected installed `workmanager 0.10.9` and `workmanager_apple 0.9.10`, specifically the latter's `WorkmanagerPlugin.swift` periodic registration, plugin callback and `BGAppRefreshTaskRequest` submission paths, plus installed Flutter 3.47 engine headers defining `FlutterImplicitEngineBridge.applicationRegistrar` and its `messenger()` accessor. The admission channel is intentionally narrow and confirms only a pending scheduler request. iOS may delay or never deliver it; physical release-device execution after suspension/restart remains required.

The controller's iOS prerequisite `DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer xcrun --sdk iphoneos --show-sdk-version` returned **69** because the Xcode license is unaccepted. No license acceptance, sudo, SDK bypass, Pods regeneration or full iOS compile was attempted. Full native compile/link plus physical Android/iOS system Calendar/Contacts behavior remain external gates.

The new platform-channel CRUD regression proves the **Contacts** adapter path only. Calendar reconciliation is reached through the same production `projectEnabled` entry point but the native Calendar adapter was not exercised by this channel fake. Its provider-specific CRUD/permission behavior and both providers' real background delivery are not established by these tests. No physical provider claims are made.

Web cross-tab serialization uses Web Locks when present; browsers without it retain same-tab coalescing and the existing durable IndexedDB attempt semantics, without a new cross-tab lock fallback. Secure token storage retains the existing read-before-replace contract, not a new cross-isolate transaction primitive.

## Changed files and self-review

- `apps/web-frontend/src/lib/api/cloud.ts`
- `apps/web-frontend/src/lib/auth/refresh-coordination.test.mjs`
- `apps/dav-bridge-mobile/lib/src/services/background_sync_service.dart`
- `apps/dav-bridge-mobile/lib/src/state/bridge_controller.dart`
- `apps/dav-bridge-mobile/test/bridge_controller_test.dart`
- `apps/dav-bridge-mobile/test/background_scheduling_test.dart`
- `apps/dav-bridge-mobile/ios/Runner/AppDelegate.swift`
- `apps/dav-bridge-mobile/ios/Runner/Info.plist`
- this report

Self-review checked matching-account guards before hydration and after the network await, credential-before-projection ordering, opt-ins read after encrypted sync, no calls to projection enable/permission request from the runner, retained revocation queue, bounded retry semantics, native/Dart task identifier agreement, and Android's independent scheduling path. Independent code review and final Android/browser acceptance are owned by the controller.

## Fixround 1 — review remediation

Base: `08a996193bddd83f29e89d0c72de06e39d6fc0e9`. Review: `/private/tmp/kamori-audit-20261015/task-6-review.md`. All three Important findings and the Minor diagnostic-capture finding were reproduced and addressed. The following supersedes the original no-Web-Locks fallback and read-before-replace limitations above. The controller explicitly authorized the narrow Rust/FRB extension and regenerated bindings required for cross-engine coordination; no dependencies were added.

### Corrected behavior and coordination contract

- Web refresh fails closed with a non-401 coordination error when Web Locks are unavailable, before bootstrap, request-ID allocation, or refresh HTTP. Existing authentication is preserved. A regression imports two independent production clients and verifies neither sends a request. Supported browsers retain the existing durable per-generation IDs and genuine revocation 401 behavior.
- Mobile foreground operations and the entire background hydration → encrypted sync → credential commit → projection sequence acquire one process-global Rust Tokio mutex. This covers the shared native runtime as well as the storage/projection intervals identified by review. Every public controller operation acquires independently; nested implementation helpers are private and unlocked. A second concurrent call on the same controller/bridge queues rather than treating the active lease as reentrancy.
- The production coordinator is a FRB opaque lease containing an `OwnedMutexGuard` in a once-initialized cell. Dart first creates the empty handle synchronously, attaching its normal FRB NativeFinalizer, then awaits acquisition into that existing handle. This avoids returning a newly acquired opaque resource to an engine that has already shut down. An empty handle cannot execute stateful native operations. Each stateful Rust async API owns a clone of the opaque lease for the complete native Future; Dart `finally` disposes only its own reference. The guard cannot be manually taken/unlocked while a native operation still holds it. Disposal or normal engine teardown therefore cannot expose shared native state while an already-started FRB operation is running. Dropping an owner while acquisition waits also eventually releases the acquired guard without orphaning it.
- Inspected the pinned Workmanager engines and app manifests: Android creates a `FlutterEngine(applicationContext)` and has no separate `android:process` override; iOS creates another Flutter engine in the same process. These engines share the native library's process-global mutex. The Dart-only queue exists solely in the debug mock/test boundary and is not the production cross-isolate mechanism.
- Accepted UX tradeoff: logout, account switch, and disable actions can wait for an active network/projection operation. After those awaited actions finish, the old coordinated operation has finished. No timeout releases a lease around an unfinished Future.
- Scheduling errors now append to existing offline/revocation warnings. The expected fake-provider error is captured and asserted rather than printing a caught stack in successful test output.

### RED → GREEN and lifecycle regression evidence

- Two-client/no-Web-Locks production coordination regression: RED **5 pass / 1 fail**, observing `Session expired. Sign in again.` instead of a coordination error. GREEN **6 pass**, no emitted network request and no auth-clear callback.
- Queued revocation plus offline provisioning plus scheduling failure: RED lost the revocation warning (`Expected contains revocation`, actual only scheduling warning). GREEN retains all three warnings, the offline snapshot, and the durable revoke queue.
- Twelve deterministic production runner/controller races: pause after snapshot read, after credential read, or immediately before projection; independently invoke logout, account switch, background disable, or collection disable. RED **all 12 failed** because the foreground action completed during the paused old operation (`Expected false, actual true`). GREEN all 12 queue the mutation and verify final account/token/preferences plus no old provider work after action completion. A separate same-controller sync/logout test proves independent concurrent calls do not bypass acquisition.
- Rust lease scaffolding without exclusion: RED **2 behavioral failures** at `Poll::Pending` assertions. GREEN four tests prove mutual exclusion/drop release, in-flight native retention after Dart-owner disposal, owner drop while acquisition waits, and rejection of unacquired handles before polling the operation. The retention regression executes the production `with_mobile_session` helper. Initial Rust opaque-path/trait setup errors were compilation setup issues, not behavioral RED evidence.
- Generated encode/decode ownership was inspected: Dart encodes a cloned opaque reference and the Rust async wrapper owns it through completion. All stateful native APIs carry the lease; independent pure crypto/device-key generation remains outside it.

### Final affected gates

Commands ran from the worktree. Bun was pinned to 1.3.14 as above. Rust commands used `PATH=/Users/yujin/.cargo/bin:$PATH CARGO_TARGET_DIR=/private/tmp/kamori-audit-target-197 rustup run 1.97.1 ...`; actual compiler was `rustc 1.97.1 (8bab26f4f 2026-07-14)` and Clippy was 0.1.97. Codegen used the installed `flutter_rust_bridge_codegen` under that toolchain and committed its output.

- `bun run --filter web-frontend check`: exit 0, **0 errors / 0 warnings**; `task6-fixround1-web-check.log`.
- `bun run --filter web-frontend test:unit`: exit 0, **66 passed / 0 failed**, 145 expectations, 11 files; `task6-fixround1-web-unit.log`.
- `bun run --filter web-frontend build`: exit 0; `task6-fixround1-web-build.log`. Existing empty `index-server` / `env` chunk notices remain.
- `cargo test -p crypto-core-lib --features frb`: exit 0, **69 unit + 14 DAV conformance tests passed**, 0 doc tests; `task6-fixround1-rust-frb.log`.
- `cargo clippy -p crypto-core-lib --features frb --all-targets -- -D warnings`: exit 0; `task6-fixround1-clippy.log`.
- `cargo fmt --all -- --check` and explicit `rustfmt --edition 2024 --check packages/crypto-core-lib/src/frb_api.rs` (the latter is included source): exit 0. Generated import ordering was formatted after codegen.
- `flutter analyze --no-pub`: exit 0, **No issues found**; `task6-fixround1-flutter-analyze.log`. An initial analyzer-only run found eight test brace lint findings; corrected before the full suite.
- `flutter test --no-pub --coverage`: exit 0, **68 tests passed**; `task6-fixround1-flutter-tests.log`. This was the one full Flutter suite in this fixround. Updated raw LCOV copied to `/private/tmp/kamori-audit-20261015/flutter-final.lcov`; this replaces the original report's raw artifact/counts. Normalized metrics remain controller-owned.
- `git diff --check`: exit 0.

All named logs are under `/private/tmp/kamori-audit-20261015/`. No Android rebuild, browser deployment acceptance, or iOS compilation was duplicated; those remain controller/external gates.

### Scope and evidence limits

Changed in this fixround: the web API and refresh coordination test; mobile controller, background runner, Rust bridge interface/FRB implementation/debug mock, and controller tests; Rust `frb_api.rs`; generated Rust binding and four generated Dart binding files; this report. Controller-owned plan and coverage-metric files are excluded from this commit.

The Rust lifecycle guarantee applies to FRB native in-flight operations retaining the lease. Third-party Contacts/Calendar platform calls do not carry that opaque lease. A provider operation already submitted when its Flutter engine is torn down may finish independently; these tests do not establish abnormal-engine provider atomicity, remote wipe, or an OS transaction spanning plugins. SPEC §9 permits interrupted projection followed by reconciliation, and the controller accepted this precise evidence boundary. Normal awaited operations are serialized and tested against completed foreground mutations. The channel CRUD regression still proves Contacts only; native Calendar CRUD and real provider/scheduler delivery remain physical-device gates. iOS SDK compilation is still blocked by the unaccepted Xcode license. The existing scheduler admission check proves pending-request admission, not delivery.

Self-review additionally checked no reentrant active-bridge bypass, every controller public async shared-state entrypoint, finally-based disposal on early exits/errors, retained native ownership across asynchronous completion, abandoned queued acquisition, absence of new permission prompts, and absence of shared-store mutations outside the coordinated production entrypoints.
