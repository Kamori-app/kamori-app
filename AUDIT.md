# Kamori implementation audit

Date: 2026-08-16

This audit records the implementation state of the hosted-beta MVP. The
normative product boundary is `SPEC.md`; this file is an evidence and release-
risk summary, not a second specification.

## Implemented security and consistency boundaries

- The cloud data plane is the signed, encrypted, idempotent `/operations`
  protocol. The former event graph and server-sequence conflict model were
  removed.
- Security spaces, not workspaces, are the cryptographic and authorization
  boundary. Writes require an active owner/editor device at the current key
  epoch.
- Operation signatures and AEAD associated data bind every public envelope
  field and ciphertext. Server sequence is allocated transactionally and is
  used only as a catch-up cursor.
- OPAQUE covers registration, login, password change, reauthentication, and
  recovery. Refresh reuse detection revokes the session family. Web refresh
  uses HttpOnly cookies with CSRF and Origin/Referer checks.
- Account recovery requires the 24-word data kit, rotates the OPAQUE record,
  returns recovery-wrapped space keys, and revokes prior sessions, passkeys,
  devices, and key packages.
- Blobs use random per-space identifiers, authenticated access, ciphertext
  hash validation, storage/egress accounting, and configurable hard breakers.
- Telemetry, crash reporting, and marketing consent are independent opt-in
  choices. Operational metrics exclude dynamic user/content identifiers.
- Web secrets and content are encrypted in IndexedDB; desktop and mobile use
  SQLCipher with platform-protected key material. Durable outboxes survive
  offline operation and retries.
- Mobile has no localhost DAV server. Android/iOS use an explicit, one-way,
  opt-in system projection. Desktop alone exposes the loopback DAV adapter with
  a dedicated random credential.

## Verification evidence from this workspace

The following checks passed on 2026-08-16:

- `bun install --frozen-lockfile`;
- `bun run verify` for all six workspace packages;
- `cargo fmt --all -- --check`;
- `cargo check --workspace --all-targets`;
- `cargo clippy --workspace --all-targets -- -D warnings`;
- `cargo test --workspace`;
- `cargo test -p crypto-core-lib --features frb`;
- `go vet ./...` and `go test ./...` in `infra`;
- `bun run build` (six of six packages);
- `cargo build --locked -p cloud-server --release`;
- Tauri release bundle generation for macOS;
- Android release APK generation with Rust libraries for arm64-v8a,
  armeabi-v7a, and x86_64;
- `bun audit --production` and `cargo audit` with no known vulnerabilities;
- Dockerfile lint, deploy-script syntax validation, YAML parsing, and
  `git diff --check`.

Backend integration tests ran against PostgreSQL and include cookie refresh,
CSRF/origin enforcement, recovery revocation, one-time TOTP backup codes, and
signed-operation authorization/idempotency.

## Accepted dependency constraints

- `rusqlite 0.39` is the newest version whose `libsqlite3-sys` linkage can be
  resolved in the same workspace as SQLx 0.9. `rusqlite 0.40` requires an
  incompatible native SQLite linkage.
- OPAQUE currently requires compatibility aliases for SHA-2 0.10 and Rand 0.8;
  the rest of the application uses their current major versions.
- RustSec reports no vulnerabilities. Its informational warnings come from
  Tauri's Linux GTK3 dependency chain (unmaintained GTK3 Rust bindings and the
  upstream GLib iterator advisory). They remain visible in the scheduled audit
  rather than being silently suppressed.
- The latest direct Bun, Flutter, Cargo, and Pulumi dependencies compatible
  with the selected toolchains are locked. Dependabot and scheduled Bun/RustSec
  audits track future updates.
- Flutter 3.47 warns that the current latest `device_calendar_plus` and
  `workmanager` Android plugins still apply the legacy Kotlin Gradle plugin.
  Current builds pass; upgrades remain automated so their built-in Kotlin
  migrations can be adopted when released.

## External release gates, not code-completion claims

The repository is ready for hosted-beta qualification, not a declaration that
a public production service already exists. These gates require credentials,
infrastructure, devices, or legal authority outside this workspace:

1. Run the PostgreSQL failover, PITR, blob restore, egress-breaker, and
   disposable hosted end-to-end exercises in `docs/runbooks` and record their
   evidence.
2. Run the CalDAV/CardDAV compatibility matrix against the explicitly supported
   third-party clients.
3. Execute the iOS native build in GitHub Actions/full Xcode and complete
   signing/TestFlight only after an Apple account and provisioning assets exist.
4. Build all containers in GitHub Actions. The local Docker command targets a
   Podman VM that was not running during this audit; individual release builds
   and Dockerfile lint passed locally.
5. Install signed desktop/Android artifacts on clean supported systems and
   complete the release checklist before publishing the draft release.
6. Replace template operator/legal details, obtain legal review of the license
   split and contributor agreement, and generate third-party notices before
   the first public source or store release.
7. Keep registration closed until two operator security keys, alert delivery,
   quotas, backups, external uptime checks, and the active-account cap are all
   proven on the hosted stack.

These gates intentionally do not weaken the service or silently enable a
public beta. Deployment and publication remain explicit human actions.
