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

- [ ] Reproduce the wrong-adapter behavior with a focused test before editing. Current Linux CI has 92 passed / 3 failed in this file; the broad PlatformException matcher also permits a false positive.
- [ ] Explicitly register/select the real appropriate federated Workmanager adapter in tests, independently of dart:io.Platform, using upstream plugin API.
- [ ] Restore test-global state after each test; assert iOS admission is called only after one native registration and Android skips the iOS channel.
- [ ] Strengthen channel-error assertions to require the exact configured PlatformException code/message and actual check count; unexpected assertions must not satisfy the test.
- [ ] Keep both positive and rejected-admission cases meaningful; no production plugin fallback.
- [ ] Run flutter analyze and flutter test; save exact commands/output in the task report. If emulating Linux selection locally, state exactly what is simulated and leave real Linux validation to GitHub.
- [ ] Self-review and commit only task files plus this plan, under mobile scope; report source/test counts and limitations.

### Task 2: Clear dependency audit failures with maintained packages

**Files:** package.json/bun.lock; desktop Cargo.toml, Rust common/auth/window commands and registration; desktop tauri.ts/TopNav.svelte and existing UI fixture/spec; Cargo.lock; docs/security/dependency-exceptions.md for dated graph status only if needed. Other production files require a concrete dependency-boundary reason in the task report.
**Inputs:** Rust primary-source diagnosis /private/tmp/kamori-pr-fixes-20261018/rust-deps-diagnosis.md and GitHub audit run 39117486416.
**Produces:** Reproducible locked graph passing existing production Bun/RustSec policy.

- [ ] Record baseline failures of bun audit --production and bash scripts/cargo-audit.sh. Do not add tests duplicating package-manager audit logic.
- [ ] Resolve devalue to at least 5.9.3 and source-map-js to at least 1.2.2. Prefer a narrow lock update compatible with all parent ranges; existing root override pattern is permitted if needed.
- [ ] Resolve quinn-proto to at least 0.11.18 and rustls to at least 0.23.46. Avoid unrelated ecosystem upgrades.
- [ ] Remove both zbus5 roots using keyring-core1.0.0 and explicit native stores: apple-native-keyring-store1.0.2 with keychain feature, windows-native-keyring-store1.1.0, and Linux dbus-secret-service-keyring-store1.0.1 with crypto-rust + vendored. Remove keyring wrapper entirely from all target manifests, initialize the corresponding real store once before Entry creation, and preserve every service/account value, serialized payload and failure behavior. See primary-source diagnosis for exact APIs and reverse paths. No production mock/default plaintext fallback.
- [ ] Remove tauri-plugin-opener and its registration; use open5.4.1-compatible with shellexecute-on-windows for the existing browser-login launch, preserving pending-login cleanup on launch failure. Keep the existing fixed LICENSE link working via a narrow fixed-URL application command and existing IPC adapter, rather than exposing unrestricted shell opening. Add real production-component IPC expectations and negative launch/error controls at existing test seams; do not weaken assertions.
- [ ] Verify maintained Linux Secret Service compatibility through an isolated test session when feasible: service/username search attributes, persistence across adapter reopen, read/write/delete, and unavailable/locked-store error propagation. An old-namespace credential seeded by secret-tool is suitable and avoids reintroducing unmaintained Rust dev dependencies. No actual user keychain reads/writes. Distinguish compiled platform paths from executed integration evidence.
- [ ] Regenerate lockfiles with pinned tooling, verify frozen/locked install, and run unchanged audit scripts. No new ignored advisory or global gate weakening.
- [ ] Run affected frontend checks/tests/builds, Rust workspace/FRB checks as appropriate to the real source delta, and meaningful native boundary regression tests for any backend changes. Report unavailable environment checks honestly; GitHub CI remains final Linux/platform evidence.
- [ ] Self-review and commit task changes with exact evidence report. No push from worker.

## Integration and acceptance

Controller independently reviews each task against its brief and diff; scoped fixes receive scoped rereview. After task review, inspect any additional failed jobs and fix the actual root cause in a new bounded task if needed.
Run final whole-change review, push fix/audit-remediation normally, merge its head into fix/e2ee-trust-boundaries without rebase/force push, and verify the documentation-only PR delta. Follow GitHub Actions to completion and update both PR descriptions to describe current verification. Preserve clean local checkout and original branches; no PR merge.
