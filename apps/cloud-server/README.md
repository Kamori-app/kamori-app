# cloud-server

Axum backend for Kamori zero-knowledge sync and sharing.

## What This Service Does

`cloud-server` is the remote API used by desktop, mobile, and web clients.
It stores encrypted data and protocol metadata, but does not decrypt user payloads.

Main responsibilities:
- OPAQUE sign-up/sign-in protocol endpoints.
- TOTP setup/status/disable endpoints for enrolled users.
- Account-recovery code regeneration and account-recovery password reset endpoints.
- Optional passkey (WebAuthn) login and management endpoints.
- Access/refresh session lifecycle endpoints (refresh, logout, revoke).
- Space-scoped encrypted blob storage with integrity and hard quota admission.
- Signed idempotent operation-log append/read endpoints for sync.
- Invite-code creation/redeem endpoints for security-space sharing.
- Workspace primitives (personal/team workspaces and member listing).
- Explicit workspace/security-space ownership transfer with recipient acceptance.
- Isolated security-key + TOTP operator control plane with audited mutations.

## Architecture

High-level components:
- HTTP layer: bootstrap/router wiring in [`src/app`](./src/app), feature routers/handlers in [`src/features`](./src/features).
- Auth/session layer: OPAQUE + JWT + optional TOTP and passkey helpers.
- State store: Valkey/Redis-backed short-lived protocol/session state.
- Persistence: PostgreSQL via `sqlx`, schema in [`migrations`](./migrations).
- Transport format: MessagePack (`application/msgpack`) for active API payloads.

Current auth storage model:
- `users` stores identity and OPAQUE profile metadata.
- `workspaces` and `workspace_members` store workspace ownership and membership roles.
- `account_recovery_codes` stores only SHA-256 hashes of one-time recovery codes (`code_hash`, `used_at`).
- `user_passkeys` stores WebAuthn credentials (`credential_id`, serialized passkey data, and `encrypted_name`).
- `security_spaces` and `security_space_members` define the crypto and authorization boundary.
- `security_space_invites` stores hash-only invite records (no plaintext invite code).
- `refresh_tokens` stores refresh-session records (`token_hash`, `expires_at`, `revoked_at`, `replaced_by_token_id`, client metadata).
- `operation_log` stores signed encrypted envelopes and server-assigned per-space cursors.
- `space_blobs` and `blob_egress_reservations` enforce storage and download budgets.

## API Surface (Current)

Core routes:
- `GET /health`
- `GET /metrics` (requires the operational metrics bearer token)
- `POST /auth/signup/start`
- `POST /auth/signup/finish`
- `POST /auth/password/change/start`
- `POST /auth/password/change/finish`
- `POST /auth/account-recovery/start`
- `POST /auth/account-recovery/finish`
- `POST /auth/signin/start`
- `POST /auth/signin/finish`
- `POST /auth/refresh`
- `POST /auth/logout`
- `POST /auth/revoke`
- `POST /auth/totp/status`
- `POST /auth/totp/setup/start`
- `POST /auth/totp/setup/finish`
- `POST /auth/totp/disable`
- `POST /auth/account-recovery/codes/regenerate`
- `POST /auth/passkey/add/start`
- `POST /auth/passkey/add/finish`
- `GET /auth/passkeys`
- `POST /auth/passkey/update`
- `POST /auth/passkey/delete`
- `POST /auth/passkey/login/start`
- `POST /auth/passkey/login/finish`
- `GET /users/me/deletion-status`
- `POST /users/me/delete`
- `GET /users/me/consents`
- `POST /users/me/consents`
- `POST /spaces`
- `GET /spaces`
- `GET /spaces/trash`
- `DELETE /spaces/{space_id}`
- `POST /spaces/{space_id}/restore`
- `GET /spaces/{space_id}/members`
- `GET /spaces/{space_id}/devices`
- `POST /spaces/{space_id}/blobs`
- `GET /spaces/{space_id}/blobs/{blob_id}`
- `POST /operations`
- `GET /operations?space_id=...&since=...`
- `POST /invite-codes`
- `POST /invite-codes/redeem`
- `POST /workspaces`
- `POST /workspaces/list`
- `POST /workspaces/members`
- `POST /workspaces/members/role`
- `POST /workspaces/members/revoke`
- `POST /ownership-transfers`
- `GET /ownership-transfers/incoming`
- `GET /ownership-transfers/outgoing`
- `POST /ownership-transfers/{transfer_id}/accept`
- `DELETE /ownership-transfers/{transfer_id}`

Operator routes use a separate identity/session namespace under `/admin-api`:
- bootstrap enrollment, security-key + TOTP login, reauthentication, and logout;
- aggregate dashboard and job heartbeats;
- versioned runtime quota/registration settings;
- account suspension and append-only operator audit;
- enrollment of additional directly attested roaming security keys.

The operator API has no content, key-recovery, or impersonation route.

Direct member-id invites are intentionally removed. Sharing is invite-code-only.

Workspace primitives contract:
- `POST /workspaces` (authenticated): creates a team workspace with caller as `owner`.
- `POST /workspaces/list` (authenticated): lists active workspaces for caller.
- `POST /workspaces/members` (authenticated): lists active members for a workspace (caller must be an active member).
- `POST /workspaces/members/role` (authenticated): updates non-owner roles with management rules (`owner/admin` only); ownership moves only through an accepted transfer.
- `POST /workspaces/members/revoke` (authenticated): revokes target member (`owner/admin` only, owner protected).
- On signup, server provisions a personal workspace (`kind=personal`) for the new user.

Operation-log contract:
- clients append signed, encrypted, idempotent envelopes to a security space;
- the server assigns monotonic `space_seq`; this cursor is transport order, not CRDT causality;
- caller device, space membership, role, key epoch, signature, and payload limits are validated.

Invite-code contract:
- client sends `invite_code_hash` (domain-separated SHA-256, 32 bytes), not plaintext code;
- server stores the hash, encrypted key package, role, optional encrypted note, and TTL metadata;
- TTL is validated server-side (`15..10080` minutes);
- redeem adds one active security-space membership without broadening access to other spaces.

Passkey login contract:
- `POST /auth/passkey/login/start` starts discoverable passkey auth and returns `flow_id`.
- `POST /auth/passkey/login/finish` accepts `flow_id` + `credential`.

Passkey management contract:
- `POST /auth/passkey/add/start` starts authenticated passkey registration and returns `flow_id`.
- `POST /auth/passkey/add/finish` completes registration and stores `encrypted_name` (client-encrypted label bytes).
- `GET /auth/passkeys` lists user passkeys (`id`, `credential_id`, `encrypted_name`).
- `POST /auth/passkey/update` updates `encrypted_name` for a passkey id.
- `POST /auth/passkey/delete` deletes a passkey by id.

Auth policy:
- account registration is OPAQUE-only (`/auth/signup/*`).
- password change is OPAQUE-only (`/auth/password/change/*`) and requires authenticated access token.
- password change finish revokes all refresh sessions for the user.

TOTP management contract:
- `POST /auth/totp/status` (authenticated) returns:
  - `available`: whether TOTP feature is enabled in server config,
  - `enabled`: whether current user has TOTP enrolled.
  - `recovery_codes_remaining`: number of unused account recovery codes.
- `POST /auth/totp/setup/start` (authenticated):
  - requires `KAMORI_ENABLE_TOTP=true`,
  - rejects when TOTP already enabled for the user,
  - returns `manual_entry_key` and `otpauth_uri`.
  - server does not return QR image payload (`qr_svg`); clients render QR locally from `otpauth_uri`.
- `POST /auth/totp/setup/finish` (authenticated):
  - requires `manual_entry_key` + current TOTP `code`,
  - verifies code with RFC6238 (HMAC-SHA1, 6 digits, 30s step),
  - stores secret to `users.totp_secret` only after successful verification,
  - generates 8 one-time account recovery codes and returns them once.
- `POST /auth/totp/disable` (authenticated):
  - idempotent when already disabled,
  - when enabled, requires a valid current TOTP `code` to clear `users.totp_secret`.
- Password signin finish (`POST /auth/signin/finish`) supports `totp_code` only when TOTP is enabled.

Account recovery contract:
- `POST /auth/account-recovery/codes/regenerate` (authenticated):
  - revokes all previous unused account recovery codes,
  - generates and returns a new set of 8 one-time account recovery codes.
- `POST /auth/account-recovery/start` (unauthenticated):
  - requires `username`, one-time `recovery_code`, and OPAQUE `opaque_start_request`,
  - consumes the matching unused account recovery code,
  - returns `opaque_server_message` + short-lived `recovery_token`.
- `POST /auth/account-recovery/finish` (unauthenticated):
  - requires `recovery_token` + OPAQUE `opaque_finish_request`,
  - updates user password file (`users.opaque_record`),
  - disables TOTP (`users.totp_secret = NULL`),
  - revokes all refresh sessions for that user.

Token model (current):
- Access token (`access_token`) is a JWT for authenticated requests.
  - Access JWT embeds `username` claim (in addition to `sub`/`kind`) so auth services can avoid repeated `SELECT username` in hot paths.
  - Authenticated request context is modeled as `principal` (`user_id + username`) extracted from access JWT.
  - Missing/blank `username` claim is treated as invalid token (`401`).
- `PreAuth` and `AccountRecovery` JWTs also include `username`; account-recovery finish requires this claim.
- Refresh token is opaque random bytes (URL-safe base64); server stores only `sha256(refresh_token)` as `token_hash`.
- Refresh transport is selected via `X-Kamori-Refresh-Transport` header:
  - `body` (default): refresh token is returned/accepted in MessagePack body.
  - `cookie`: refresh token is stored/read via HTTP-only cookie; body omits refresh token.
- Cookie mode CSRF protection (web-only):
  - server sets non-HttpOnly CSRF cookie (`__Host-kamori_csrf` by default),
  - `POST /auth/refresh` and `POST /auth/logout` require `X-Kamori-Csrf-Token` header matching CSRF cookie.
  - cookie-mode `POST /auth/refresh` and `POST /auth/logout` also validate request `Origin` (or `Referer`) against `KAMORI_CORS_ALLOW_ORIGINS`.
- On successful password/passkey login server returns `access_token` and sets/returns refresh according to transport mode.
- `POST /auth/refresh` rotates refresh token in one transaction (one-time use):
  - old token gets `revoked_at=now` and `replaced_by_token_id=<new_id>`;
  - new refresh token row is created and returned/set with a new access token.
- Reuse/invalid/expired refresh token returns unauthorized.
- `POST /auth/logout` revokes current refresh session (idempotent) and clears refresh cookie in cookie mode.
- `POST /auth/revoke` revokes another refresh session by `refresh_token_id` for same owner (idempotent).
- Client contract: refresh tokens must be stored in platform secure storage (Keychain/Keystore/secure enclave equivalent), not plaintext local storage.

## Runtime Configuration

Required for production:
- `KAMORI_JWT_SECRET` (must be non-empty and not placeholder `change-me`).

Important settings:
- `KAMORI_DATABASE_URL` (Postgres URL)
- `KAMORI_DATABASE_MAX_CONNECTIONS`
- `KAMORI_BIND_ADDR` (default `127.0.0.1:8080`)
- `KAMORI_VALKEY_URL`
- `KAMORI_VALKEY_KEY_PREFIX`
- `KAMORI_VALKEY_TTL_SECONDS`
- `KAMORI_REGISTRATION_ENABLED` (default `false`)
- `KAMORI_BETA_ACCOUNT_LIMIT` (default `1000`)
- `KAMORI_MAX_BLOB_BYTES` (default `26214400`)
- `KAMORI_ACCOUNT_STORAGE_BYTES` (default `5000000000`)
- `KAMORI_OWNER_MONTHLY_EGRESS_BYTES` (default `10000000000`)
- `KAMORI_OWNER_ROLLING_24H_EGRESS_BYTES` (default `2000000000`)
- `KAMORI_GLOBAL_NONESSENTIAL_EGRESS_STOP_BYTES` (default `16000000000000`)
- `KAMORI_GLOBAL_EMERGENCY_EGRESS_BREAKER_BYTES` (default `19000000000000`)
- `KAMORI_OBJECT_STORE_ENDPOINT` (required; include `https://` for B2/S3)
- `KAMORI_OBJECT_STORE_REGION` (required)
- `KAMORI_OBJECT_STORE_BUCKET` (required; private ciphertext bucket)
- `KAMORI_OBJECT_STORE_ACCESS_KEY_ID` (required secret)
- `KAMORI_OBJECT_STORE_SECRET_ACCESS_KEY` (required secret)
- `KAMORI_OBJECT_STORE_ALLOW_HTTP` (default `false`; local MinIO only)
- `KAMORI_OBJECT_STORE_VIRTUAL_HOSTED_STYLE` (default `false`; path-style by default)
- `KAMORI_METRICS_BEARER_TOKEN` (required, at least 32 bytes)
- `KAMORI_ENABLE_TOTP`
- `KAMORI_WEBAUTHN_RP_ID`
- `KAMORI_WEBAUTHN_RP_ORIGIN`
- `KAMORI_WEBAUTHN_RP_NAME`
- `KAMORI_ADMIN_WEBAUTHN_RP_ORIGIN` (exact operator-console origin)
- `KAMORI_ADMIN_WEBAUTHN_RP_NAME`
- `KAMORI_ADMIN_TOTP_KEK` (standard-base64 encoding of exactly 32 random bytes)
- `KAMORI_AUTH_TOTP_KEK` (a different standard-base64 32-byte key for consumer TOTP seeds)
- `KAMORI_JWT_ISSUER`
- `KAMORI_JWT_AUDIENCE`
- `KAMORI_ACCESS_TOKEN_TTL_SECONDS` (default `300`, i.e. 5 minutes)
- `KAMORI_REFRESH_TOKEN_TTL_SECONDS` (default `2592000`, i.e. 30 days)
- `KAMORI_JWT_PREAUTH_TTL_SECONDS`
- `KAMORI_JWT_ACCOUNT_RECOVERY_TTL_SECONDS` (default `600`)
- Web refresh-cookie settings:
  - `KAMORI_WEB_REFRESH_COOKIE_NAME` (default `__Host-kamori_rt`)
  - `KAMORI_WEB_REFRESH_COOKIE_PATH` (default `/`)
  - `KAMORI_WEB_REFRESH_COOKIE_DOMAIN` (optional, default unset)
  - `KAMORI_WEB_REFRESH_COOKIE_SECURE` (default `true`)
  - `KAMORI_WEB_REFRESH_COOKIE_SAMESITE` (`lax`/`strict`/`none`, default `lax`)
  - `KAMORI_WEB_CSRF_COOKIE_NAME` (default `__Host-kamori_csrf`)
  - Note: `__Host-` cookie names require `Secure=true`, `Path=/`, and no `Domain`.
  - Local HTTP dev note:
    - set `KAMORI_WEB_REFRESH_COOKIE_SECURE=false`,
    - set non-`__Host-` names, for example:
      - `KAMORI_WEB_REFRESH_COOKIE_NAME=kamori_rt`
      - `KAMORI_WEB_CSRF_COOKIE_NAME=kamori_csrf`
    - or use `apps/cloud-server/.env.web-dev.example` as a baseline.

CORS:
- `KAMORI_CORS_ALLOW_ORIGINS`
- `KAMORI_CORS_ALLOW_METHODS`
- `KAMORI_CORS_ALLOW_HEADERS` (default includes `x-kamori-refresh-transport`, `x-kamori-csrf-token`)
- `KAMORI_CORS_ALLOW_CREDENTIALS` (default `true`)

CORS safety rule:
- `KAMORI_CORS_ALLOW_CREDENTIALS=true` cannot be combined with wildcard origin (`*`).

## Local Development

Prerequisites:
- Rust toolchain pinned by root `rust-toolchain.toml`
- PostgreSQL
- Valkey/Redis-compatible server
- Optional: `sqlx-cli` for migrations

Install `sqlx-cli`:

```bash
cargo install sqlx-cli --no-default-features --features postgres,rustls
```

Apply migrations:

```bash
cd apps/cloud-server
export KAMORI_DATABASE_URL="postgres://user:pass@localhost:5432/kamori"
cargo run -p cloud-server -- migrate
```

Current migration baseline:
- `20260301_0001_init.sql` contains account, session, workspace, and recovery primitives.
- `20260816_0002_signed_oplog.sql` adds devices, security spaces, invites, and the signed operation log.
- `20260816_0003_space_blobs_and_quotas.sql` adds space-scoped blob metadata and egress reservations.
- `20260816_0004_user_consents.sql` adds independent, default-off consent choices and their audit trail.

Run in dev:

```bash
cd /path/to/repo
export KAMORI_DATABASE_URL="postgres://user:pass@localhost:5432/kamori"
export KAMORI_VALKEY_URL="valkey://127.0.0.1:6379/0"
export KAMORI_JWT_SECRET="replace-with-strong-secret"
export KAMORI_OBJECT_STORE_ENDPOINT="http://127.0.0.1:9000"
export KAMORI_OBJECT_STORE_REGION="us-east-1"
export KAMORI_OBJECT_STORE_BUCKET="kamori-local"
export KAMORI_OBJECT_STORE_ACCESS_KEY_ID="local-minio-key"
export KAMORI_OBJECT_STORE_SECRET_ACCESS_KEY="local-minio-secret"
export KAMORI_OBJECT_STORE_ALLOW_HTTP="true"
export KAMORI_METRICS_BEARER_TOKEN="replace-with-at-least-32-random-bytes"
bun run --filter cloud-server dev
```

## Quality and Test Commands

```bash
bun run --filter cloud-server fast
bun run --filter cloud-server lint
bun run --filter cloud-server test
```

Where:
- `fast` = `cargo check -p cloud-server --tests` (quick local feedback).
- `lint` = strict clippy gate (`-D warnings`).
- `test` = full `cargo test -p cloud-server` (first cold run can be slow).

Transport note:
- Active API endpoints use MessagePack (`application/msgpack`).

## Build

Binary:

```bash
cargo build -p cloud-server --release
```

Docker image:

```bash
docker build -f apps/cloud-server/Dockerfile -t kamori/cloud-server:local .
```

## Production Deployment

### Local container smoke test

```bash
docker run --rm -p 8080:8080 \
  -e KAMORI_BIND_ADDR=0.0.0.0:8080 \
  -e KAMORI_DATABASE_URL='postgres://user:pass@db:5432/kamori' \
  -e KAMORI_VALKEY_URL='valkey://valkey:6379/0' \
  -e KAMORI_JWT_SECRET='replace-with-strong-secret' \
  -e KAMORI_ADMIN_TOTP_KEK='replace-with-standard-base64-32-byte-key' \
  -e KAMORI_OBJECT_STORE_ENDPOINT='https://s3.eu-central-003.backblazeb2.com' \
  -e KAMORI_OBJECT_STORE_REGION='eu-central-003' \
  -e KAMORI_OBJECT_STORE_BUCKET='kamori-primary' \
  -e KAMORI_OBJECT_STORE_ACCESS_KEY_ID='replace-with-b2-key-id' \
  -e KAMORI_OBJECT_STORE_SECRET_ACCESS_KEY='replace-with-b2-application-key' \
  -e KAMORI_METRICS_BEARER_TOKEN='replace-with-at-least-32-random-bytes' \
  -e KAMORI_CORS_ALLOW_ORIGINS='https://app.example.com' \
  kamori/cloud-server:local
```

### Native binary

- Ship `target/release/cloud-server`.
- Provide env vars via systemd/unit secrets manager.
- Run migrations before rolling out new binary.
- Put the service behind reverse proxy/TLS.

The hosted beta does not use a standalone API container command. The release
workflow builds API, user web, operator console, and edge images, pins each by
digest, migrates once, and rolls both app nodes using
`deploy/cloud-server/compose.yaml` with whole-release rollback.

## Operational Notes

- Service fails fast on insecure/missing JWT secret in non-test runs.
- Schema migrations are operator-controlled (not auto-run on startup).
- The production image supports `cloud-server migrate`; rollout automation runs it once before replacing app nodes.
- The production image supports `cloud-server healthcheck` for an internal readiness probe without adding HTTP tools to the runtime image.
- PostgreSQL stores blob authorization, integrity, and quota metadata only; ciphertext bytes live in the configured private S3-compatible bucket.
- Blob downloads are served through the authenticated API so strict owner and global egress reservations happen before delivery.
- `/metrics` exposes aggregate process counters only, has no user/content labels, and requires a separate bearer secret.
- Invite codes are redeemed transactionally and can only be used while valid.
