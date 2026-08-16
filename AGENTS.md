# Repository Guidelines

## Project Structure & Module Organization
Kamori is a monorepo using Bun workspaces + Turborepo and a Rust Cargo workspace.
- `apps/cloud-server/`: Rust Axum backend (`src/`, SQL migrations in `migrations/`).
- `apps/dav-bridge-desktop/`: desktop bridge UI + Tauri wrapper (`src-tauri/`).
- `apps/dav-bridge-mobile/`: Flutter mobile bridge (`lib/`, tests in `test/`).
- `packages/crypto-core-lib/`: shared Rust crypto/runtime library (WASM + FRB surface).
- `SPEC.md`: consolidated specification and historical iteration notes.

## Product Constraints (Current)
- Registration is web-portal only. Desktop and mobile clients are sign-in-only.
- Sharing uses short-lived invite codes (generate + redeem), not direct member id input in UI.
- Invite expiry is user-selectable from 15 minutes through 7 days.
- API transport is MessagePack for active endpoints; binary payload fields should stay bytes-oriented (`serde_bytes`/`ByteBuf`) instead of `Vec<u8>` JSON arrays.
- Local SQLite cache paths are fixed:
  - Desktop: `.kamori/local-cache.sqlite3`
  - Mobile: `.kamori/mobile-cache.sqlite3`
- Mobile target platforms are Android + iOS only.
- Web and mobile are first-party offline PIM clients for calendars, tasks, and
  contacts. Desktop is the CalDAV/CardDAV bridge and control center.
- Mobile clients do not run a localhost DAV server. Android/iOS system
  calendar/contact integration is explicit opt-in per collection.
- The canonical cloud model is a signed encrypted operation log. DAV is a
  local desktop projection, not the server data model.

## Build, Test, and Development Commands
From repo root:
- `bun install`: install JS workspace dependencies.
- `bun run dev`: run all `dev` tasks via Turbo.
- `bun run build`: run workspace build pipeline.
- `bun run fast`: quick local Rust loop for `cloud-server` + `crypto-core-lib` (`cargo check --tests` path).
- `bun run verify`: full strict workspace verification (`bun run lint && bun run test`).
- `bun run lint`: run workspace lint pipeline.
- `bun run test`: run workspace test pipeline.
- Tooling baseline: `bun@1.3.14` (root `packageManager` pin).

Rust-focused commands:
- `cargo fmt --all -- --check`: formatting check (CI-enforced).
- `cargo test --workspace`: full Rust test sweep.
- `cargo clippy --workspace --all-targets -- -D warnings`: strict Rust lint gate.
- `cargo test -p crypto-core-lib --features frb`: verify FRB-enabled core build path.
- `bun run --filter cloud-server fast`: quick compile/tests-check loop for backend.
- `bun run --filter crypto-core-lib fast`: quick compile/tests-check loop for FRB core.

Flutter:
- `cd apps/dav-bridge-mobile && flutter pub get`
- `cd apps/dav-bridge-mobile && flutter analyze`
- `cd apps/dav-bridge-mobile && flutter test`
- `cd apps/dav-bridge-mobile && flutter create . --platforms=android,ios --overwrite` (only when regenerating platform scaffolding)

Web frontend (targeted):
- `bun run --filter web-frontend check`
- `bun run --filter web-frontend test:unit`
- `bun run --filter web-frontend build`

FRB codegen:
- `flutter_rust_bridge_codegen generate` (from repo root with current config)
- Regenerated bindings in `apps/dav-bridge-mobile/lib/src/rust/gen/` and `packages/crypto-core-lib/src/frb_generated.rs` are committed.

## Coding Style & Naming Conventions
- Rust: follow `rustfmt` defaults (4-space indentation), keep clippy warnings at zero.
- Flutter/Dart: use `flutter_lints`; prefer single quotes and `const` constructors/literals where possible.
- Naming:
  - Rust modules/files: `snake_case`
  - Dart types: `PascalCase`
  - SQL migrations: `YYYYMMDD_NNNN_description.sql`

## Testing Guidelines
- Add Rust unit tests near implementation with `#[cfg(test)]` and explicit names (for example, `rejects_invalid_nonce`).
- Prefer API model round-trip tests for transport changes (JSON/MessagePack).
- For mobile changes, run both `flutter analyze` and `flutter test`.
- For FRB surface changes, ensure `cargo test -p crypto-core-lib --features frb` passes.
- For invite-code flow changes, test both server-side normalization/TTL validation and mobile controller redeem/create paths.

## Mobile Workflow Guardrails
- Do not run `flutter create` in repository root.
- Run `flutter create` only in `apps/dav-bridge-mobile`.
- Use `--platforms=android,ios` to avoid generating unsupported platform directories.
- Re-check `pubspec.yaml`, `lib/main.dart`, and `test/widget_test.dart` after `flutter create` because Flutter can overwrite app-specific files.
- Flutter debug builds default to mock Rust bridge unless overridden. For real FRB runtime in debug, pass:
  - `flutter run --dart-define=KAMORI_USE_MOCK_RUST=false`
- Production and integration builds must use the real packaged Rust bridge and
  must never fall back to the mock runtime.
- Do not add a mobile localhost CalDAV/CardDAV server. Use Android
  Calendar/Contacts providers and iOS EventKit/Contacts for optional system
  projection.

## Commit & Pull Request Guidelines
Use clear imperative commit messages, scoped by component, for example:
- `cloud-server: enforce jwt secret fail-fast`
- `mobile: replace unsupported frb methods with real transport`

PRs should include:
- concise behavioral summary,
- linked issue/spec when available,
- commands executed and outcomes,
- screenshots/logs for UI/runtime-impacting changes.
