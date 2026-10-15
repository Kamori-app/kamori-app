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
