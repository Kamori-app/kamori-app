# crypto-core-lib

Shared Rust core library for Kamori cryptography, signed operation sync, and local bridge runtime.

## What This Module Does

`crypto-core-lib` is the common implementation consumed by:
- `dav-bridge-desktop` (Tauri Rust side),
- `dav-bridge-mobile` (through FRB bindings),
- web integration paths (WASM-capable feature set).

Main responsibilities:
- cryptographic primitives and envelope operations,
- security-space key wrapping/unwrapping for sharing,
- signed encrypted operation envelopes and durable offline outbox,
- local bridge runtime for DAV/cache operations,
- mobile FRB API surface.

Token contract implemented in core runtime:
- `access_token` is used for authenticated cloud operations.
- `refresh_token` is opaque and used only for `POST /auth/refresh`.
- Local bridge client retries once on `401`, rotates refresh token, then retries original request with new access token.
- Mobile FRB exposes refresh-token runtime controls: import/export/clear for secure client-side persistence wiring.
- Local bridge runtime supports optional workspace scope via `LocalBridgeConfig::with_workspace_id(...)`.
  - When omitted, sync/push uses personal workspace behavior on server.

## Architecture

Main areas:
- `CryptoEngine` in `src/lib.rs`
  - X25519 key exchange
  - AES-256-GCM / XChaCha20-Poly1305 payload encryption
  - group key wrapping/unwrapping
- invite-code helpers in `frb_api`
  - normalized code generation
  - SHA-256 code hashing
  - collection-key wrap/unwrap with invite-derived key
- `operation_envelope.rs` and `pim.rs`
  - canonical signatures, authenticated encryption, and versioned PIM operations
- `local_bridge_runner/`
  - local SQLite cache and DAV handlers
  - cloud sync via MessagePack transport
- FRB API (`feature = "frb"`)
  - source API in `src/lib.rs` (`mod frb_api`)
  - generated glue in `src/frb_generated.rs`

## Cargo Features

- `native` (default)
- `http-reqwest` (default)
- `local-bridge` (default)
- `wasm` for wasm/web build paths
- `frb` for Flutter bridge exports

## WASM OPAQUE Exports (Web)

When built with `--features wasm --no-default-features`, the library exports browser-usable OPAQUE helpers:

- `opaque_signup_start(password)`
- `opaque_signup_finish(flow_id, password, opaque_server_message)`
- `opaque_signin_start(password)`
- `opaque_signin_finish(flow_id, password, opaque_server_message)`
- `generate_qr_svg(payload)` for local QR rendering (used by web TOTP setup).

These OPAQUE helpers are reused by web flows for:
- sign-up/sign-in,
- authenticated password change,
- account recovery password reset (server-side recovery-code verification).

The web frontend consumes these bindings via:
- `apps/web-frontend/src/lib/opaqueClient.ts`
- `apps/web-frontend/src/lib/wasm/crypto-core-lib/*` (generated)

## Local Development

Run core tests:

```bash
cargo test -p crypto-core-lib
```

The default test run includes `tests/dav_conformance.rs`, a black-box suite
that starts the embedded listener on an operating-system-assigned loopback
port and exercises it through HTTP. It covers dedicated Basic Auth, discovery,
CalDAV/CardDAV resource lifecycle, conditional writes and deletes, query and
multiget reports, and RFC 6578 sync tokens/tombstones. It does not replace the
release compatibility matrix against real third-party DAV clients.

Run only that suite:

```bash
cargo test -p crypto-core-lib --test dav_conformance
```

Run FRB-enabled tests:

```bash
cargo test -p crypto-core-lib --features frb
```

Passkey enrollment/login uses platform credential-provider wiring in each client;
the shared core owns protocol payload handling only.

Lint:

```bash
bun run --filter crypto-core-lib lint
```

Fast local loop:

```bash
bun run --filter crypto-core-lib fast
```

This runs `cargo check -p crypto-core-lib --features frb --tests`.

Format:

```bash
cargo fmt --all -- --check
```

## Build

Debug:

```bash
bun run --filter crypto-core-lib build
```

Release:

```bash
cargo build -p crypto-core-lib --release
```

FRB build path:

```bash
cargo build -p crypto-core-lib --features frb
```

WASM build path:

```bash
CARGO_TARGET_DIR=/tmp/kamori-wasm-target \
  cargo build -p crypto-core-lib --target wasm32-unknown-unknown --features wasm --no-default-features
```

Generate web bindings:

```bash
wasm-bindgen \
  --target web \
  --out-dir apps/web-frontend/src/lib/wasm/crypto-core-lib \
  /tmp/kamori-wasm-target/wasm32-unknown-unknown/debug/crypto_core_lib.wasm
```

## Production / Deployment Notes

`crypto-core-lib` is not deployed as a standalone network service.
It is shipped inside client artifacts (desktop/mobile) and linked into those builds.

For release discipline:
1. Regenerate FRB bindings when FRB API changes.
2. Run `cargo test -p crypto-core-lib --features frb`.
3. Rebuild dependent apps (`dav-bridge-desktop`, `dav-bridge-mobile`) before shipping.
