# web-frontend

SvelteKit web frontend for Kamori (landing + web app).

## What This App Does

`web-frontend` is the browser client for Kamori.
It contains:
- landing page (`/`),
- web app area (`/app`) for auth, collections, sync stats, and invite-code sharing.

Main responsibilities:
- Full OPAQUE auth flow against `cloud-server` (start/finish executed in browser runtime).
- OPAQUE password-change flow in web settings (`/auth/password/change/*`).
- TOTP setup/disable UI in web settings (QR generated locally in browser from `otpauth_uri` + manual key fallback).
- Account recovery-code UI for:
  - authenticated regeneration in settings,
  - unauthenticated account recovery in sign-in modal.
- Passkey login flow against `cloud-server` (discoverable flow - no username input required).
- Passkey management API support with client-encrypted passkey labels.
- Client-side collection keys, device keys, materialized PIM state, and durable
  outbox encrypted at rest in IndexedDB.
- Invite-code generation/redeem with client-side code handling.
- MessagePack transport to backend APIs.
- Documentation-first landing sections (`Why`, `What`, `How`, `Downloads`, `Compatibility`, `Security`, `Sharing`, `FAQ`).

Local DAV runtime and local SQLite are out of scope for web.

## Token Usage

- `access_token` is attached as Bearer token to authenticated API requests.
- `refresh_token` is used only for `POST /auth/refresh` when access token is expired.
- Web app uses cookie transport (`X-Kamori-Refresh-Transport: cookie`): refresh token is read/written only via `HttpOnly` cookie.
- Web app sends `X-Kamori-Csrf-Token` from CSRF cookie for cookie-mode `POST /auth/refresh` and `POST /auth/logout`.
- Web app retries once on `401` by rotating refresh cookie, then repeats the original request with new `access_token`.
- Only non-sensitive UI descriptors and sync counters are stored in
  `localStorage`; key material and content use the encrypted IndexedDB vault.
- `access_token` / preauth token are memory-only in web app runtime.
- `refresh_token` is not accessible from JS state (`HttpOnly` cookie only).
- The account master key is memory-only unless the user explicitly approves
  local passkey unlock; that path wraps it with a non-extractable WebCrypto key.
- Changing cloud base URL clears in-memory auth tokens and requires re-login.
- If server uses non-default CSRF cookie name, set `VITE_KAMORI_WEB_CSRF_COOKIE_NAME`.

Server-side token TTL policy (cloud-server env):
- `KAMORI_ACCESS_TOKEN_TTL_SECONDS` (default `300`)
- `KAMORI_REFRESH_TOKEN_TTL_SECONDS` (default `2592000`)
- `KAMORI_JWT_ACCOUNT_RECOVERY_TTL_SECONDS` (default `600`)

## TOTP UX

- TOTP management is available in web settings modal (`/app`).
- Setup flow:
  - call `POST /auth/totp/setup/start`,
  - receive `manual_entry_key` + `otpauth_uri`,
  - generate QR locally in browser from `otpauth_uri`,
  - confirm via `POST /auth/totp/setup/finish` with one current code,
  - receive 8 one-time account recovery codes (displayed once, user must save them).
- Recovery-code regeneration:
  - call `POST /auth/account-recovery/codes/regenerate` (revokes old unused codes and returns new 8 codes).
- Disable flow:
  - call `POST /auth/totp/disable` with current TOTP code.
- Security note:
  - QR is generated client-side only; `otpauth_uri` is never sent to third-party QR services.

## Account Recovery UX

- Recovery is available in Sign In modal via one-time account recovery code.
- Flow:
  - call `POST /auth/account-recovery/start` with `username`, `recovery_code`, and OPAQUE `opaque_start_request`,
  - call `POST /auth/account-recovery/finish` with `recovery_token` + OPAQUE `opaque_finish_request`.
- On success:
  - password is reset,
  - TOTP is disabled,
  - user sees explicit notice to sign in again with new password or passkey.

## Architecture

Layers:
- SvelteKit app shell + routing.
- UI pages/components under `src/routes` and `src/lib`.
- API transport clients in `src/lib/api` (MessagePack encode/decode).
- Non-sensitive browser state store in `src/lib/stores/app.ts`.
- Encrypted IndexedDB vault and outbox in `src/lib/cryptoVault.ts`.
- PIM operation/materialization helpers in `src/lib/pim.ts`.
- OPAQUE wasm wrapper in `src/lib/opaqueClient.ts`.
  - includes local `generate_qr_svg(payload)` helper used by TOTP setup UI.
- Generated Rust wasm-bindgen artifacts in `src/lib/wasm/crypto-core-lib/`.

Current adapter:
- `@sveltejs/adapter-node` in [`svelte.config.js`](./svelte.config.js).

## API Dependencies

This app uses `cloud-server` endpoints:
- `/auth/signup/*`
- `/auth/password/change/start`
- `/auth/password/change/finish`
- `/auth/account-recovery/start`
- `/auth/account-recovery/finish`
- `/auth/signin/*`
- `/auth/totp/status`
- `/auth/totp/setup/start`
- `/auth/totp/setup/finish`
- `/auth/totp/disable`
- `/auth/account-recovery/codes/regenerate`
- `/auth/passkey/add/start`
- `/auth/passkey/add/finish`
- `/auth/passkeys`
- `/auth/passkey/update`
- `/auth/passkey/delete`
- `/auth/passkey/login/start`
- `/auth/passkey/login/finish`
- `/invite-codes`
- `/invite-codes/redeem`
- `/devices`
- `/spaces`
- `/operations?space_id=...&since=N`
- `/users/me/consents`

Base URL comes from `VITE_KAMORI_API_BASE_URL`, with
`http://127.0.0.1:3000` used only as the development fallback.

## Local Development

From repository root:

```bash
bun install
bun run --filter web-frontend dev
```

Default dev URL:
- `http://localhost:4173`

## Regenerating OPAQUE WASM Bindings

The web app uses generated bindings from `packages/crypto-core-lib` with `feature = "wasm"`.

One-time setup:

```bash
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.114
```

From repository root:

```bash
CARGO_TARGET_DIR=/tmp/kamori-wasm-target \
  cargo build -p crypto-core-lib --target wasm32-unknown-unknown --features wasm --no-default-features

wasm-bindgen \
  --target web \
  --out-dir apps/web-frontend/src/lib/wasm/crypto-core-lib \
  /tmp/kamori-wasm-target/wasm32-unknown-unknown/debug/crypto_core_lib.wasm
```

Generated files:
- `src/lib/wasm/crypto-core-lib/crypto_core_lib.js`
- `src/lib/wasm/crypto-core-lib/crypto_core_lib.d.ts`
- `src/lib/wasm/crypto-core-lib/crypto_core_lib_bg.wasm`
- `src/lib/wasm/crypto-core-lib/crypto_core_lib_bg.wasm.d.ts`

## Quality and Test Commands

```bash
bun run --filter web-frontend lint
bun run --filter web-frontend check
bun run --filter web-frontend run test:unit
bun run --filter web-frontend test
bun run --filter web-frontend build
```

Current unit tests cover section navigation and cookie refresh/CSRF session
behavior. Backend and crypto-core suites cover the matching MessagePack,
OPAQUE, recovery, signed-operation, and key-wrapping contracts.

## Build

```bash
bun run --filter web-frontend build
bun run --filter web-frontend preview
```

## Production Deployment

The production build uses `adapter-node` and runs as the hosted Kamori web service.

The supported deployment is the repository's non-root Node container behind
the hosted edge proxy. A static adapter is not a supported release path.

Release checklist:
1. Build the app in CI (`bun run --filter web-frontend build`).
2. Verify CORS policy on `cloud-server` allows web origin.
3. Validate auth + invite-code flows against production backend.
