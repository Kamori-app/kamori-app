# dav-bridge-desktop

Desktop bridge application for Kamori (Tauri v2 + Svelte 5).

## What This App Does

`dav-bridge-desktop` is the desktop client shell around the local DAV runtime.
It signs users in to `cloud-server`, runs the localhost DAV bridge, and exposes sync/collection controls.

Main responsibilities:
- OPAQUE password sign-in and passkey sign-in entrypoints.
- Passkey sign-in opens the trusted web origin for discoverable WebAuthn and
  returns through the short-lived device-authorization flow; the Tauri WebView
  never claims the `kamori.app` WebAuthn origin.
- Local bridge lifecycle control (`start`, `stop`, `status`, manual and periodic sync).
- Copy-safe DAV setup details and dedicated credential rotation.
- Strict conditional DAV mutations: existing resources require a matching
  strong ETag and concurrent writes cannot silently overwrite each other.
- Local collection management (`create`, `list`).
- Desktop window/tray behavior preferences.

Current UX scope:
- Registration is handled in `web-frontend` (desktop app is sign-in-only).
- Password change and account recovery are handled in `web-frontend`.
- Sharing via invite codes is handled in web/mobile clients.

## Architecture

Runtime layers:
- UI layer: Svelte SPA in [`src`](./src).
- Desktop shell: Tauri app in [`src-tauri`](./src-tauri).
- Core runtime: `crypto-core-lib::local_bridge_runner`.
- API transport: MessagePack.

Key Rust command modules:
- [`commands/auth.rs`](./src-tauri/src/commands/auth.rs)
- [`commands/bridge.rs`](./src-tauri/src/commands/bridge.rs)
- [`commands/collections.rs`](./src-tauri/src/commands/collections.rs)
- [`commands/window.rs`](./src-tauri/src/commands/window.rs)
- [`commands/session.rs`](./src-tauri/src/commands/session.rs)

Security/behavior notes:
- Tauri command ACL is explicit in [`src-tauri/permissions/default.toml`](./src-tauri/permissions/default.toml).
- SQLite cache path is fixed (non-editable): `.kamori/local-cache.sqlite3`.
- Hide-on-close keeps Dock icon visible by design (macOS upstream behavior constraints).

## Token Usage

- `access_token` is used for authenticated cloud API calls.
- `refresh_token` is used only to refresh expired access tokens (`POST /auth/refresh`), with rotation on every refresh.
- Desktop runtime keeps `access_token` in Tauri in-memory state and passes it to `crypto-core-lib::local_bridge_runner`.
- Desktop stores the `refresh_token` and its random per-generation refresh
  attempt id together in OS secure storage (keychain via `keyring`) and keeps a
  runtime copy in memory while the app is running. A legacy token-only keychain
  entry is upgraded before the next network refresh.
- Rotated `refresh_token` from local runtime is synced back into keychain/state during sync/status/stop command paths.
- Space provisioning uses the current recovery package and starts sync at the
  later of the membership-history boundary and current-epoch snapshot cursor,
  so a new desktop never needs superseded epoch keys.
- Changing backend URL clears in-memory tokens and removes old backend refresh token from keychain.

Server-side token TTL policy is configured in `cloud-server`:
- `KAMORI_ACCESS_TOKEN_TTL_SECONDS` (default `300`)
- `KAMORI_REFRESH_TOKEN_TTL_SECONDS` (default `2592000`)

## Local Development

Prerequisites:
- Bun `1.3.14`
- Rust toolchain
- Tauri prerequisites for your OS

Install dependencies:

```bash
bun install
```

Run frontend-only dev server:

```bash
bun run --filter dav-bridge-desktop dev
```

Run full desktop app in dev:

```bash
bun run --filter dav-bridge-desktop tauri:dev
```

## Build

Frontend bundle:

```bash
bun run --filter dav-bridge-desktop build
```

Desktop bundle/package:

```bash
bun run --filter dav-bridge-desktop tauri:build
```

Artifacts are produced by the workspace target directory in
`../../target/release/bundle/*`.

## Validation Commands

```bash
bun run --filter dav-bridge-desktop lint
cargo test -p dav-bridge-desktop
cargo clippy -p dav-bridge-desktop --all-targets -- -D warnings
```

## Production Release / Deployment

The tag-driven GitHub Actions release workflow builds signed Windows and
notarized macOS artifacts, Linux bundles, and a signed self-hosted Flatpak
repository. Exact credentials and approval gates are documented in
[`docs/runbooks/client-release.md`](../../docs/runbooks/client-release.md).

Configuration is app-driven (backend URL and behavior preferences are set in UI/settings).
