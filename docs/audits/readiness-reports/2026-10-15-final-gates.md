# Final integration evidence

Continuation baseline `dc13f1909cc9acff3e13d8fe849a26943efe17ce`; branch `fix/audit-remediation`, same visible checkout. Application tree `07d1721`, native production inputs `341d976`; the later native change in Task7 is cfg(test)-only. Task8 `0b1cc7f` changes restore tooling/runbook only. Logs and private synthetic artifacts: `/private/tmp/kamori-readiness-20261015`.

## Completed parent gates

Commands in this completed package/native table exited0. Rust uses1.97.1 and the dedicated PostgreSQL18.6 database; Bun1.3.14, Flutter3.47/Dart3.13, Xcode27.0(27A266a)/SDK27, installed simulator runtime iOS26.2. No global xcode-select changes.

| Command / boundary | Actual result | Log |
|---|---|---|
| `cargo test --workspace` |262 entries: server162/default core65/DAV15/corpus2/desktop18;0 ignored |final-rust-workspace.log|
| `cargo test -p crypto-core-lib --features frb` |80 units+15 DAV+2 corpus=97; overlaps workspace population |final-rust-frb.log|
| `cargo fmt --all -- --check` |PASS |final-rust-fmt.log|
| `cargo clippy --workspace --all-targets -- -D warnings` |PASS |final-rust-clippy.log|
| Frontend Turbo test/build, three packages, `--force` |3 test tasks+3 builds/0 cache hits; web74 units/254 assertions+4 mounted; admin4 units+2 UI; desktop3 UI+2 web TTL |final-frontends-test.log, final-frontends-build.log|
| `flutter analyze`, `flutter test` |0 issues,95 tests PASS |final-flutter-analyze.log, final-flutter-test.log|
| Real-FRB simulator integration, projection disabled |3 PASS/3 expected disabled-provider skips, permissions remain notDetermined |final-ios-default-off-prepared.log|
| Real installed providers, explicit grants to own test bundle |6/6 PASS |final-ios-providers.log|
| Real-FRB production `lib/main.dart` simulator host, sequential XCTest |Host PASS,3/3 XCTest PASS |final-ios-production-host.log, final-ios-production-tests.log/.xcresult|
| `scripts/with-ios-toolchain.sh flutter build ios --release --no-codesign --dart-define=KAMORI_USE_MOCK_RUST=false` |81.8s Xcode;47,703,013-byte Runner.app, arm64 Runner/linked Rust framework,10 matching FRB/lease symbols |final-ios-device-release.log|
| `flutter build apk --release --dart-define=KAMORI_USE_MOCK_RUST=false` |219.3s Gradle;102,725,051-byte APK; packaged actual ELF arm64/armv7/x86_64 verified |final-android-release.log|

Rust populations overlap; 359 unique tests is not a valid total. The isolated native timeout child is included in its feature suite, rather than a second independent test scenario. Existing negative quota test emits expected ERROR; measured workload internal errors remain0.

Android APK SHA256 `8d6ac52e90c87fdc7aae3fd54cdcf8ab02f07a2afa48df6034bd5e2caa45443f`, preserved as `native-artifacts/app-release-341d976.apk`. ELF e_machine armv7=40/arm64=183/x86_64=62. iOS Runner SHA256 `308a2184ad7fb4a6cd79569ad61cc1275b8921981f59f916de050e0594631501`; Rust framework SHA256 `410f441069c95a22816f353b53cb496cffcc7e04a086bafd4b9b2dc964c7d7fc`. Runner.app remains in the local mobile build; detailed native manifests in `native-artifacts/{ios,android}-341d976.json`.

The first controller iOS command incorrectly used `--no-pub` after other Flutter graph generation: generated Package.swift had0 dependencies and missing workmanager_apple caused a build failure. Normal pub/plugin generation restored6 dependencies and passed. Failed log retained as `final-ios-default-off.log`. Initial nm probe inspected Runner; the correct linked Rust framework contained the exports and passed nm/file/otool checks. These are harness corrections, not product RED findings. Native operations ran sequentially once shared Flutter/SPM generated state was identified.

## First fresh-image full acceptance

`bun run acceptance:full`, source `0b1cc7f`, `KAMORI_ACCEPTANCE_SKIP_BUILD=false`, `COMPOSE_PROJECT_NAME=kamori-readiness-20261015`, `CI=` (no retries): **exit1,28 PASS/1 FAIL**,1.5min tests. Full cloud/web/admin builds ran before the suite and only this disposable project's volumes were reset. Existing valid Docker layers may be reused; old running containers were replaced.

| Image | Immutable image ID | Observed container ID |
|---|---|---|
| cloud-server |2fa21cae60f4da730ca8c248612c453e56100d59fa33b6afa8cc3790f89b2981 |4fdb2146964d611abe886f0642ad49567de32206170ece04e7ad4ae5dece3ee1|
| web |797ff868023ae318fdab1ebb88cee3d71838a0830034fea85b56c3f52f6957bf |622c254ce55a3e4f5dcf89a712b80a452273bfd2c17ce45a9d1f5f96d5d5d71f|
| admin |b394fd0915276ae5fb1f990fd8c1bcb3ade10911c34ac8b604feba8175c41a8f |4902ae6458fd370c680eb13eb4cac0f5b25bdfb0e7df5bc543ba173931be8f65|

Failure: `tests/acceptance/specs/smoke.spec.ts:98` creates an event on2030-01-02 and, after sign-in, expects it in current week's Agenda without date navigation. Task6's Agenda is the specified seven-day view. The exact test/product-contract correction is assigned to final review; the encrypted persistence assertion must be preserved. Log `final-acceptance.log`, full original trace/video/error context preserved in `acceptance-initial-artifacts`. No automatic retry or passing full-suite claim.

The29 cases consist of9 deployed-stack scenarios and20 mounted production-component scenarios using synthetic IPC/HTTP/authenticator peers (Task4 nine,Task5 seven,Task6 four). They must not all be described as full-stack encrypted synchronization tests.

## Fresh full acceptance after the combined correction

Source `b85f70c7ae3b12a7512d8e9104ebfb8b5a4df573`: **exit0,43 passed (1.4min), no retries**, `KAMORI_ACCEPTANCE_SKIP_BUILD=false`, all three images built and own stack recreated. Log `final-acceptance-corrected.log`; snapshots/report preserved in `acceptance-b85f70c-artifacts`; immutable image/container IDs matched and saved in `acceptance-b85f70c-images.json`. Cloud image remained byte-identical to the earlier runtime; web changed to `b1dd4a8b2799ee13ec94ac4de76ef328ca5559815415f9ddf78597d7144c71d2`, admin to `d2b1f034f87bc1124914538aba00fe7a411fa70e56705f52948037e04a394150`.

The43 cases are9 deployed-stack scenarios and34 mounted production-component cases with synthetic peers:23durability (old9 +14new),7desktop/admin/invite UI,4recurrence. The new critical case is @smoke; current PR smoke selects3cases, while the original9durability cases and helper/auth cases remain full-only.

Scoped review accepted C1/R16, I1 and M1 with no new Critical/Important breakage. It found MinorN1: three stale UI catches bypass the guarded notice path; at that b85 stage the bounded follow-up remained pending per decision21. It is completed at e3c86d5 below, followed by the successful current-image full run. [Scoped report](2026-10-15-final-review-round-1.md), [combined fix evidence](2026-10-15-final-fix.md).

## Final frozen-source acceptance and closure

Source `e3c86d5f7f5a690c14d686bac0d045907d009917`: **exit0,45 passed (1.4min), retries0**, `KAMORI_ACCEPTANCE_SKIP_BUILD=false`. All three image builds ran; only the owned project was reset/recreated. Current immutable image IDs were matched to the actual running containers before cleanup and committed in [final-acceptance.json](2026-10-15-final-acceptance.json). Log `final-acceptance-final.log`, full artifacts `acceptance-e3c86d5-artifacts`.

| Runtime image | Immutable ID |
|---|---|
|cloud|2fa21cae60f4da730ca8c248612c453e56100d59fa33b6afa8cc3790f89b2981|
|web|98b74e6cb6e33f4f3c367564caf31b1374cc09a0167ee048afa89a02893ab9df|
|admin|d1ab3cf0118c847ba50085a58d8601c7f8a38456b10341c3362153c2cd0051fa|

Population:9 deployed scenarios and36 mounted component scenarios with synthetic peers (25durability/7UI/4recurrence). The new N1 late-notification test and current-error/editor-rejection control pass; C1's critical @smoke regression remains selected. [Final scoped review](2026-10-15-final-review-round-2.md) closes N1 with no new issue. All original final findings C1/I1/M1 were accepted in round1; no confirmed finding remains open.

Final input comparison `341d976..e3c86d5` for mobile/core/native wrapper/Cargo/FRB configuration finds only `packages/crypto-core-lib/src/frb_api/credential_tests.rs`, an existing cfg(test)-only change. Server/admin/desktop comparison `07d1721..e3c86d5` finds only the cfg(test) readiness marker assertion. Native/server production inputs therefore retain the earlier parent-gate provenance; changed web inputs have current check/74units/25durability and final production image/full-runtime evidence. No unrelated native/Go/restore suite was repeated.

[Cleanup](2026-10-15-cleanup.json): after ownership preflight, removed exact9 own containers/3 volumes/project network, synthetic runtime files and the dedicated shutdown simulator. Independent final inventories for parent and restore namespaces are empty. Existing images, user caches/build outputs and private reports are preserved. Branch stays `fix/audit-remediation`; protected original ref stays8124892. All21 ledger rulings are archived [verbatim](2026-10-15-rulings.md).

## Limits and final status

Whole-continuation review, combined corrections, bounded N1 follow-up, both scoped reviews and final frozen-source full acceptance are complete; no confirmed finding remains open. Task8 actual encrypted PITR/DR evidence is separately committed in its harness evidence/report; it is not a remote-provider or application-sync certification. No fresh line/branch coverage measured; retain historical revision-qualified snapshots.

Unsigned iOS packaging is not a signed IPA or physical execution. Android packaging is not Android device execution. Scheduler delivery, physical power loss, secure-store/data-protection behavior, external DAV-client matrix, signing/TestFlight, remote production DR/RPO/RTO remain external release exercises. Reported existing toolchain/dependency notices, codegen INFO and empty Svelte chunks are triaged in final review. User caches/build artifacts/images are retained; only this run's exact resources were removed.
