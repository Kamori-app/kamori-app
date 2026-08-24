# web-frontend

SvelteKit web frontend for Kamori (landing + web app).

## What This App Does

`web-frontend` is the browser client for Kamori.
It contains:
- landing page (`/`),
- routed web app area (`/app`) for authentication, offline PIM, devices,
  sessions, spaces, automatic sync, recovery, and invite-code sharing.

Main responsibilities:
- Full OPAQUE auth flow against `cloud-server` (start/finish executed in browser runtime).
- OPAQUE password-change flow in the routed Security settings
  (`/auth/password/change/*`).
- TOTP setup/disable UI in Security settings (QR generated locally in browser
  from `otpauth_uri` + manual key fallback).
- Separate security UX for:
  - one-time TOTP backup-code regeneration in settings,
  - 24-word data recovery-kit display and account recovery.
- Passkey login flow against `cloud-server` (discoverable flow - no username input required).
- Passkey management API support with client-encrypted passkey labels.
- Client-side collection keys, device keys, materialized PIM state, and a
  causally ordered durable outbox encrypted at rest in IndexedDB.
- Invite-code generation/redeem with client-side code handling.
- MessagePack transport to backend APIs.
- English-first localized landing sections (`Product`, `How it works`, `Apps`,
  `Security`, `Questions`) with direct document and download links.

Local DAV runtime and local SQLite are out of scope for web.

## Application Routes

The authenticated app shell stays mounted while its focused views change:

- `/app` — Today overview;
- `/app/tasks`, `/app/calendar`, `/app/contacts` — first-party PIM views;
- `/app/spaces` — encrypted space creation, trash, and access entry points;
- `/app/sharing?space=<id>` — invite and membership controls for one space;
- `/app/settings/{general,security,devices,privacy,account,advanced}` — routed
  settings; the self-hosted service endpoint is isolated under Advanced.

Authentication is intentionally separate from recovery:

- `/app/sign-in` — OPAQUE or passkey sign-in;
- `/app/sign-up` — web-only account registration;
- `/app/recovery` — destructive Data Recovery Kit flow with its revocation
  effects shown before submission.

An account without a space sees a dedicated first-run screen instead of empty
task/calendar/contact forms. Write controls are also disabled for reader spaces
and devices that have not received the current space key.

## Browser Synchronization

- A full initial sync discovers spaces, key packages, members, trash, and the
  signed encrypted operation journal.
- Subsequent foreground auto-sync runs use the lighter operation-delta path and
  the stored per-space cursor. A metadata refresh runs at least every five
  minutes.
- Delta sync is requested after a local write, when connectivity returns, when
  the tab becomes visible or focused, and every 30 seconds while visible.
- Overlapping runs are coalesced, the data plane is serialized with Web Locks,
  and a separate non-waiting Web Lock avoids duplicate simultaneous polling by
  multiple tabs.
- Failures use exponential backoff with jitter up to five minutes. The header
  reports syncing, offline, pending-outbox, and persistent error states; manual
  sync remains available from that status control.
- A locked or closed browser never gives a Service Worker the account master
  key. Encrypted outbox entries resume after the user unlocks the app.

## Token Usage

- `access_token` is attached as Bearer token to authenticated API requests.
- `refresh_token` is used only for `POST /auth/refresh` when access token is expired.
- Web app uses cookie transport (`X-Kamori-Refresh-Transport: cookie`): refresh token is read/written only via `HttpOnly` cookie.
- The API keeps both refresh and CSRF cookies host-only and `HttpOnly`. Login returns the same
  CSRF value in its CORS-protected MessagePack response; after a page reload the
  web app recovers it through `POST /auth/csrf`, which requires the refresh
  cookie and an allowed `Origin`/`Referer`.
- The web app keeps that CSRF value in memory and sends it in
  `X-Kamori-Csrf-Token` for cookie-mode refresh/logout requests. It never tries
  to read an API-subdomain cookie through `document.cookie`. Refresh rotates
  both cookies; the next CSRF value is returned only to the allowed origin.
- Before refresh, the browser commits a random attempt id to a separate
  IndexedDB auth-runtime store keyed by a digest of backend origin and current
  CSRF generation. Tabs therefore share the exact retry identity, and a lost
  response can be retried after reload without deriving an id from a bearer
  credential. The record contains no refresh token or content key.
- Web app retries once on `401` by rotating refresh cookie, then repeats the original request with new `access_token`.
- Only the selected cloud origin is stored in `localStorage`; usernames,
  collection names, cursors, counters, tokens, key material, and content are not.
  Durable encrypted client state lives in the IndexedDB vault.
- Every encrypted-vault IndexedDB record, local-unlock record, and outbox sequence is scoped by
  a domain-separated SHA-256 digest of normalized cloud origin plus normalized
  username. The username is not present in IndexedDB lookup keys, and switching
  server or account cannot reuse another account's vault or pending operations.
- `access_token` and the one-time TOTP continuation are memory-only in web app runtime.
- `refresh_token` is not accessible from JS state (`HttpOnly` cookie only).
- The account master key is memory-only unless the user explicitly opts in to
  local browser unlock. That path wraps it with a non-extractable WebCrypto
  key. It protects copied browser storage, but is not hardware binding: trusted
  any code executing under that origin in the browser profile can request decryption.
- Changing cloud base URL clears in-memory auth tokens and requires re-login.

Server-side token TTL policy (cloud-server env):
- `KAMORI_ACCESS_TOKEN_TTL_SECONDS` (default `300`)
- `KAMORI_REFRESH_TOKEN_TTL_SECONDS` (default `2592000`)
- `KAMORI_JWT_ACCOUNT_RECOVERY_TTL_SECONDS` (default `600`)

## TOTP UX

- TOTP management is available at `/app/settings/security`.
- Setup flow:
  - complete a scoped OPAQUE reauthentication and call `POST /auth/totp/setup/start`,
  - receive one-time `flow_id`, `manual_entry_key` + `otpauth_uri`,
  - generate QR locally in browser from `otpauth_uri`,
  - confirm via `POST /auth/totp/setup/finish` with `flow_id` and one current code,
  - receive 8 one-time TOTP backup codes (displayed once, user must save them).
- TOTP backup-code regeneration:
  - consume a scoped reauthentication proof and call
    `POST /auth/account-recovery/codes/regenerate` (revokes old unused backup
    codes and returns 8 replacements).
- Disable flow:
  - consume a scoped reauthentication proof and call `POST /auth/totp/disable`
    with the current TOTP code.
- Security note:
  - QR is generated client-side only; `otpauth_uri` is never sent to third-party QR services.

## Account Recovery UX

- Recovery is a dedicated `/app/recovery` flow and is not embedded in Sign In.
  It uses the 24-word data recovery kit; TOTP backup codes cannot perform data
  recovery.
- Flow:
  - derive the recovery verifier and account master key locally from the words;
  - call `POST /auth/account-recovery/start` with `username`,
    `recovery_verifier`, and the OPAQUE start for the new password;
  - call `POST /auth/account-recovery/finish` with the one-time
    `recovery_token`, OPAQUE finish, and newly wrapped account master key;
  - decrypt returned current space-key packages locally and persist them in the
    encrypted vault.
- On success:
  - password is reset,
  - TOTP is disabled,
  - prior sessions, passkeys, devices, and old device packages are revoked,
  - user signs in with the new password to enroll a clean device.

## Architecture

Layers:
- SvelteKit app shell + routing.
- Structured global notifications, inline form errors, and a persistent sync
  status replace the former single bottom-of-page notice string.
- UI pages/components under `src/routes` and `src/lib`.
- API transport clients in `src/lib/api` (MessagePack encode/decode).
- Non-sensitive browser state store in `src/lib/stores/app.ts`.
- Encrypted IndexedDB vault and outbox in `src/lib/cryptoVault.ts`.
- Same-tab and Web Locks API serialization around PIM commits, sync, and key
  rotation; IndexedDB assigns a monotonic queue order that retries preserve.
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
- `/auth/reauth/*`
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
- `/spaces/recovery-key-packages`
- `/spaces/{space_id}/devices`
- `/spaces/{space_id}/members`
- `/spaces/{space_id}/rotate-key`
- `/spaces/{space_id}/members/{user_id}/revoke`
- `/spaces/{space_id}/device-key-packages`
- `/spaces/{space_id}/recovery-key-package`
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
cargo install wasm-bindgen-cli --version 0.2.127
```

From repository root:

```bash
CARGO_TARGET_DIR=/tmp/kamori-wasm-target \
  cargo build -p crypto-core-lib --release --target wasm32-unknown-unknown --features wasm --no-default-features

wasm-bindgen \
  --target web \
  --out-dir apps/web-frontend/src/lib/wasm/crypto-core-lib \
  /tmp/kamori-wasm-target/wasm32-unknown-unknown/release/crypto_core_lib.wasm
```

The `wasm-bindgen-cli` version must exactly match the `wasm-bindgen` version
resolved in `Cargo.lock`. Keep the generated browser artifact on the release
profile: debug generation makes the shipped WASM several times larger.

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

Current unit tests cover PIM operation/snapshot validation, section navigation,
auto-sync trigger coalescing, and cookie refresh/CSRF session behavior. Backend and crypto-core suites cover
the matching MessagePack, OPAQUE, recovery, signed-operation, rotation, and
key-wrapping contracts.

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
