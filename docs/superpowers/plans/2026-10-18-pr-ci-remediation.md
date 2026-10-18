# PR audit and CI remediation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task. Steps use checkbox syntax for tracking.

**Goal:** Make dependency security audit and affected CI checks for PR #122 pass, and carry the verified fixes to dependent documentation PR #123.
**Architecture:** Keep existing production behavior and audit policy. Fix host-dependent test registration; update vulnerable locked dependencies and select maintained native dependency paths with preserved behavior.
**Tech Stack:** Bun 1.3.14; Rust 1.97.1; Flutter 3.47.0; existing GitHub Actions.
**Spec:** User instruction to fix the failing PR checks; AGENTS.md and the existing security/dependency-exceptions.md policy. This is CI/dependency remediation, not implementation of the pending E2EE design.

## Global Constraints

- Work in the visible existing fix/audit-remediation branch; no new worktrees, no codex branch prefix, no history rewrite.
- User authorizes updating the two existing PR branches and pushing their fixes; no merge/deployment.
- Do not disable checks, use mocks in production, add blanket advisory ignores, downgrade security guarantees, or delete legitimate tests.
- Preserve native OS credential storage, trusted transport and desktop URL opening; new unmaintained advisories require a maintained graph solution, not silently extended exceptions.
- Bun commands use 1.3.14; Rust uses rustup run 1.97.1. Reuse existing caches; build artifacts/logs go under /private/tmp/kamori-pr-fixes-20261018.
- Existing calendar/contact fixtures intentionally use CRLF. Do not normalize them to satisfy default diff whitespace.
- Only the controller switches branches, integrates dependent heads and pushes. Implementers may commit their scoped task; no subagents from workers.
- Tests/diagnoses must separate source defects from test/environment errors and preserve evidence of both RED and GREEN.

## Review Focus

- Target platform overrides must select the intended Workmanager adapter on Linux as well as macOS.
- Error propagation must match the intended native error rather than a failed mock assertion.
- Security updates must clear actual advisory paths in the complete cross-target lockfile.
- Secure-store/URL-opening replacements must preserve identifiers, persistence and existing authorization behavior.
- Fresh dependency results must not overwrite revision-qualified historical test evidence.

### Task 1: Make background scheduling tests independent of host OS

**Files:** apps/dav-bridge-mobile/test/background_scheduling_test.dart; explicit test dependencies in pubspec.yaml/pubspec.lock if needed.
**Inputs:** /private/tmp/kamori-pr-fixes-20261018/mobile-ci.log and mobile-diagnosis.md.
**Produces:** Meaningful host-independent tests of existing iOS/Android scheduling; no production service change unless a distinct bug is proved.

- [x] Reproduce the wrong-adapter behavior with a focused test before editing. Current Linux CI has 92 passed / 3 failed in this file; the broad PlatformException matcher also permits a false positive.
- [x] Explicitly register/select the real appropriate federated Workmanager adapter in tests, independently of dart:io.Platform, using upstream plugin API.
- [x] Restore test-global state after each test; assert iOS admission is called only after one native registration and Android skips the iOS channel.
- [x] Strengthen channel-error assertions to require the exact configured PlatformException code/message and actual check count; unexpected assertions must not satisfy the test.
- [x] Keep both positive and rejected-admission cases meaningful; no production plugin fallback.
- [x] Run flutter analyze and flutter test; save exact commands/output in the task report. If emulating Linux selection locally, state exactly what is simulated and leave real Linux validation to GitHub.
- [x] Self-review and commit only task files plus this plan, under mobile scope; report source/test counts and limitations.

### Task 2: Clear dependency audit failures with maintained packages

**Files:** package.json/bun.lock; desktop Cargo.toml, Rust common/auth/window commands and registration; desktop tauri.ts/TopNav.svelte and existing UI fixture/spec; Cargo.lock; docs/security/dependency-exceptions.md for dated graph status only if needed. Other production files require a concrete dependency-boundary reason in the task report.
**Inputs:** Rust primary-source diagnosis /private/tmp/kamori-pr-fixes-20261018/rust-deps-diagnosis.md and GitHub audit run 39117486416.
**Produces:** Reproducible locked graph passing existing production Bun/RustSec policy.

- [x] Record baseline failures of bun audit --production and bash scripts/cargo-audit.sh. Do not add tests duplicating package-manager audit logic.
- [x] Resolve devalue to at least 5.9.3 and source-map-js to at least 1.2.2. Prefer a narrow lock update compatible with all parent ranges; existing root override pattern is permitted if needed.
- [x] Resolve quinn-proto to at least 0.11.18 and rustls to at least 0.23.46. Avoid unrelated ecosystem upgrades.
- [x] Remove both zbus5 roots using keyring-core1.0.0 and explicit native stores: apple-native-keyring-store1.0.2 with keychain feature, windows-native-keyring-store1.1.0, and Linux dbus-secret-service-keyring-store1.0.1 with crypto-rust + vendored. Remove keyring wrapper entirely from all target manifests, initialize the corresponding real store once before Entry creation, and preserve every service/account value, serialized payload and failure behavior. See primary-source diagnosis for exact APIs and reverse paths. No production mock/default plaintext fallback.
- [x] Remove tauri-plugin-opener and its registration; use open5.4.1-compatible with shellexecute-on-windows for the existing browser-login launch, preserving pending-login cleanup on launch failure. Keep the existing fixed LICENSE link working via a narrow fixed-URL application command and existing IPC adapter, rather than exposing unrestricted shell opening. Add real production-component IPC expectations and negative launch/error controls at existing test seams; do not weaken assertions.
- [x] Verify maintained Linux Secret Service compatibility through an isolated test session when feasible: service/username search attributes, persistence across adapter reopen, read/write/delete, and unavailable/locked-store error propagation. An old-namespace credential seeded by secret-tool is suitable and avoids reintroducing unmaintained Rust dev dependencies. No actual user keychain reads/writes. Distinguish compiled platform paths from executed integration evidence.
- [x] Regenerate lockfiles with pinned tooling, verify frozen/locked install, and run unchanged audit scripts. No new ignored advisory or global gate weakening.
- [x] Run affected frontend checks/tests/builds, Rust workspace/FRB checks as appropriate to the real source delta, and meaningful native boundary regression tests for any backend changes. Report unavailable environment checks honestly; GitHub CI remains final Linux/platform evidence.
- [x] Self-review and commit task changes with exact evidence report. No push from worker.

### Task 3: Restore public reproducible S3 acceptance fixtures

**Files:** tests/acceptance/compose.yaml and README.md; tests/release-readiness/restore/Dockerfile, run.py and README.md for the same active fixture pins. Historical evidence-2026-10-15.json remains unchanged. Workflow change only if a distinct proven orchestration defect requires it.
**Inputs:** /private/tmp/kamori-pr-fixes-20261018/container-diagnosis.md and containers-ci.log.
**Produces:** Public digest-pinned MinIO fixture and init client that work without registry credentials.

- [x] Confirm existing quay.io server and mc pins fail anonymous pull; distinguish registry authorization from application or bucket permissions.
- [x] Replace both acceptance-only image references with ghcr.io/coollabsio/minio:RELEASE.2025-10-15T17-29-55Z@sha256:69b55a1c1c5dc285ce04db96689f5b2102317fc77a50680a1874ca6efd1c87f9 after independently confirming report manifest/layer evidence.
- [x] Replace the same active MinIO server constant in restore/run.py and mc build-stage image in restore/Dockerfile, preserving /usr/bin/mc COPY and existing restore behavior. Keep historical source-qualified manifests unchanged.
- [x] Keep service names, server command, init shell/mc script, bucket permissions, synthetic credentials, ports and volume declarations unchanged. The image embeds the existing RELEASE.2025-08-13T08-35-41Z mc client and provides /bin/sh, sleep and bash.
- [x] Document community rebuild source/provenance, upstream security-release change, immutable multiarch digest and acceptance-only scope. Do not imply official MinIO provenance, production certification or security scanning not performed.
- [x] Exercise anonymous pull, init, S3 put/get and private bucket behavior on an isolated owned fixture namespace; use only synthetic data. Confirm failures remain failures; preserve init status gating.
- [x] Run affected acceptance smoke/full suite once on final package inputs if available, or rely on actual new GitHub run and state which evidence came from it. Verify the restore tooling image still builds/copies the same mc binary and run its focused orchestration controls; do not falsely reattribute historical full PITR evidence to this update.
- [x] Clean up only exact owned containers/volumes/network; retain user caches/images. Self-review, scoped commit and report with observed commands/results.

### Task 4: Bound expired zoned recurrence work on Linux

**Files:** apps/web-frontend/src/lib/pim.ts and focused recurrence tests; remediation report/plan for source-qualified results only. Change other recurrence consumers only if a demonstrated shared defect requires it.
**Input:** /private/tmp/kamori-pr-fixes-20261018/frontend-ci-new-failure.log, job117655524253, commit f0436aa. CI timed out after10s in existing long-duration expired SECONDLY recurrence test while other frontend gates passed.
**Produces:** Correct bounded recurrence evaluation under actual pinned Bun1.3.14 on Linux and macOS, retaining long-overlap and timezone semantics. Do not increase/remove timeout to mask unbounded work.

- [x] Diagnose/reproduce actual failure with pinned tooling; distinguish production algorithm defect, platform dependency and incorrect test assertion.
- [x] Add or strengthen focused RED regression evidence for the proven root cause without duplicating implementation. Preserve short expired=false and long interval still overlaps=true, UTC/positive/negative offsets, DST, UNTIL and COUNT constraints.
- [x] Implement the smallest behavior-preserving recurrence correction. No arbitrary silent truncation, unchecked empty result or weakened existing deadline.
- [x] Run focused and full web unit/check/build/UI gates, actual Linux execution when feasible; keep exact commands/output and bounds in report. Use existing isolated container caches and synthetic data only; remove exact owned runtime resources and volumes, preserve user caches/images.
- [x] Self-review, update remediation report with new finding/result, commit scoped files and report. No push/branch switching or worker subagents.

## Integration and acceptance

Controller independently reviews each task against its brief and diff; scoped fixes receive scoped rereview. After task review, inspect any additional failed jobs and fix the actual root cause in a new bounded task if needed.
Run final whole-change review, push fix/audit-remediation normally, merge its head into fix/e2ee-trust-boundaries without rebase/force push, and verify the documentation-only PR delta. Follow GitHub Actions to completion and update both PR descriptions to describe current verification. Preserve clean local checkout and original branches; no PR merge.
