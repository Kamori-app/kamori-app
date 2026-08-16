# Kamori MVP specification

Status: implementation-aligned contract
Last updated: 2026-08-16

This file describes the product implemented by this repository and the exact
boundary of the first hosted beta. Architecture rationale lives in `docs/adr`;
future work lives only in `docs/ROADMAP.md`.

## 1. Product boundary

Kamori is an end-to-end encrypted operation platform. The MVP is a personal
information manager for calendars, tasks, and contacts. DAV is an optional
desktop adapter, not the storage model.

- Web and mobile are first-party PIM clients.
- Desktop is a sign-in/collection control surface and a loopback-only
  CalDAV/CardDAV bridge.
- Android and iOS may project Kamori calendars and contacts into system stores
  only after explicit opt-in. Mobile never runs a localhost DAV server.
- The hosted service is the first deployment target. Self-host packaging is
  post-MVP, while storage and configuration boundaries remain portable.

Realtime documents, mail, federation, SSO/SCIM, a general drive, large
attachments, and enterprise policy are not MVP features.

## 2. Accounts and authentication

- Registration exists only in the web portal. Desktop and mobile sign in to an
  existing account.
- The login identifier is a canonical lowercase username, not an email
  address. The MVP has no email verification or email reset path.
- Password registration, login, password change, reauthentication, and data
  recovery use OPAQUE. Concurrent OPAQUE exchanges have independent random
  flow handles.
- Passkeys are implemented in web and desktop. Native mobile passkey plumbing
  is explicitly deferred; mobile exposes password plus optional TOTP sign-in.
- TOTP is optional. Its one-time backup codes are distinct from the data
  recovery kit and are atomically consumed.
- Access tokens are short-lived. Refresh tokens rotate, detect reuse, and can
  be listed and revoked. Web refresh transport uses HttpOnly cookies plus CSRF
  and Origin/Referer validation; native clients use body transport.
- Registration is disabled by default and additionally protected by a strict,
  operator-configurable active-account cap.

All active application endpoints use MessagePack. Binary protocol values stay
binary rather than JSON number arrays.

## 3. Devices, keys, and recovery

Every device has an Ed25519 signing key and an X25519 HPKE key. Device private
keys stay in browser encrypted IndexedDB or the platform secret store. A new
device may authenticate but cannot decrypt an existing security space until a
trusted client uploads a current device key package.

The web client provides device listing, approval, naming metadata, and
revocation. A revoked device cannot append new operations. Historical signing
keys remain discoverable when needed to verify existing log entries.

At web registration Kamori creates a 24-word BIP39 data recovery kit. The
server receives only a domain-separated verifier and account-master-key-wrapped
space packages. Recovery:

1. proves possession of the 24 words;
2. creates a new OPAQUE password record;
3. returns only the caller's current recovery-wrapped space keys;
4. disables TOTP and deletes its backup codes;
5. revokes every refresh session, passkey, device, and old device package;
6. consumes the short-lived recovery token exactly once.

The recovering web client preserves recovered space keys but creates a new
device identity on the next sign-in. There is no support bypass and no
recovery without the kit or a still-approved device.

## 4. Authorization and sharing

The hierarchy is:

`account -> workspace -> security space -> stream -> operation`

A workspace is an organizational container. A security space is the smallest
independently shared cryptographic and authorization boundary. Roles are
`owner`, `editor`, and `reader`; the server admits writes only from active
owner/editor devices at the current key epoch.

Sharing uses single-use invite codes, never direct member-id entry in client
UI. The issuer chooses an expiry from 15 minutes through 7 days and a reader or
editor role. Codes are normalized, hashed, rate-limited, and atomically
redeemed. The recipient installs both a device package and a recovery package.

Member removal requires a complete set of new-epoch packages for the remaining
members' active devices and atomically rotates the epoch. Removed parties keep
anything they previously decrypted; Kamori does not claim retroactive erasure.
Ownership transfer requires explicit offer and acceptance. Spaces enter a
30-day trash before purge, and account deletion cannot orphan shared assets.

## 5. Encrypted operation transport

The canonical envelope v1 contains random `space_id`, `stream_id`, and
`client_op_id`; `author_device_id`; `key_epoch`; envelope kind; cipher suite;
nonce; ciphertext; and an Ed25519 signature. AEAD associated data and the
signature bind every public client field and ciphertext using a
domain-separated canonical encoding.

The server validates membership, write role, active device, current epoch,
size, nonce, and signature. PostgreSQL transactionally allocates a monotonic
per-space `space_seq`. Identical retries return the original sequence;
reusing a `client_op_id` for different bytes is rejected.

`space_seq` is a catch-up cursor, not CRDT causality or a client timestamp.
The server cannot parse operation plaintext. Current clients emit versioned
field-oriented PIM upsert/delete payloads. The transport reserves `snapshot`
and `control` envelope kinds, but the MVP clients do not yet generate snapshot
or history-compaction records; the hosted beta therefore retains its operation
log instead of promising a 90-day compaction window.

Future encrypted documents may introduce a benchmarked CRDT payload codec
without changing this envelope or tying durable history to MLS messages.

## 6. Offline state and conflicts

- Web persists account-scoped encrypted keys, materialized PIM state, and an
  encrypted-operation outbox in IndexedDB. A service worker caches the shell.
- Mobile persists its SQLCipher cache, runtime hydration data, and durable
  outbox at `.kamori/mobile-cache.sqlite3`; background sync reconstructs that
  state before work.
- Desktop uses `.kamori/local-cache.sqlite3`, protected by a key held in the
  operating-system credential store.

Local writes are materialized immediately and queued before upload. A cloud
ack of the same operation is idempotent. Causally valid cloud operations
replace local provisional projections even though wall-clock time and server
sequence are different units. Concurrent upserts with an unobserved head are
materialized as visibly marked conflict copies. The MVP does not claim a full
field-by-field conflict editor or CRDT convergence for PIM data.

## 7. Current PIM scope

The first-party UI supports the practical core fields currently implemented:

- calendar event title, start, and end;
- task title and completion;
- contact name, email, and phone;
- creation, editing, deletion, offline queueing, sharing, and conflict-copy
  visibility.

The common Rust projection preserves existing unedited iCalendar/vCard fields
and unknown `X-*` properties during first-party partial edits. DAV imports keep
the original full resource bytes. Rich recurrence editing, attendee workflows,
alarms, contact photos/groups, semantic search, import/export UX, and a polished
conflict resolver are post-MVP unless separately implemented and tested before
release.

## 8. Desktop DAV bridge

The bridge binds only to an explicitly verified loopback address and requires
a dedicated random credential stored outside the account password. It supports
well-known discovery, principal/home/collection discovery, `OPTIONS`, depth
`0/1` `PROPFIND`, calendar/addressbook query and multiget `REPORT`,
`sync-collection`, `GET`/`HEAD`, conditional `PUT`, and `DELETE`. Its encrypted
SQLite change journal produces opaque sync tokens and tombstones.

`MKCOL`, `MKCALENDAR`, and `PROPPATCH` return an honest unsupported response.
Query filtering is currently broad and outbound scheduling/iMIP is absent.
Compatibility is promised only for clients in a subsequently executed release
matrix. See `docs/whitepapers/desktop-dav-bridge.md`.

## 9. Mobile system integration

Android and iOS are first-party Flutter clients backed by the real Rust core in
release builds. System calendar/contact projection is off by default, asks for
platform permission, and is one-way from Kamori in this MVP. Tasks remain
inside Kamori. Disabling projection lets the user keep or remove projected
copies. Plaintext written to a system provider leaves Kamori's E2EE boundary.

See `docs/whitepapers/mobile-system-integration.md` for the user-facing privacy
and behavior contract.

## 10. Blobs, quotas, and privacy

Blobs use random per-space IDs. The server verifies ciphertext SHA-256,
membership, padded size, and quota before writing to private S3-compatible
storage. Downloads are authenticated, metered, and streamed through the API;
there is no global hash-knowledge read path.

Default configurable beta limits are 5 GB stored ciphertext per owner,
25 MiB per padded blob, 10 GB blob egress per calendar month, 2 GB per rolling
24 hours, 1 MiB per operation/control envelope, 25 MiB per snapshot envelope,
and 1,000 active accounts. At limits, nonessential blob work is rejected while
auth, operation sync, recovery, deletion, and administration remain available.
Global nonessential and emergency egress breakers default to 16 TB and 19 TB.

Telemetry, crash reporting, and marketing consent are independent and always
opt-in. The server's built-in metrics use aggregate low-cardinality labels and
never usernames, space IDs, content, tokens, keys, or ciphertext payloads.

## 11. Operations and deployment

The hosted-beta Pulumi Go stack declares a Hetzner EU private network, two app
nodes, PostgreSQL primary/standby, an ops node, protected volumes, firewall,
and load balancer. Backblaze B2 is the primary ciphertext store and a private
Hetzner Object Storage bucket is the independent DR target. Deployment remains
an explicit operator action; CI does not provision production by default.

The repository includes container definitions, Caddy routing, Prometheus,
Grafana, Alertmanager, backup/restore and manual failover scripts/runbooks, and
GitHub Actions for verification and controlled infrastructure/deploy paths.
PostgreSQL is authoritative. Valkey contains only ephemeral auth/rate-limit
state and cannot be required for durable correctness.

The separate admin application requires an operator identity, passkey, TOTP,
and audited actions. It exposes aggregate health, quotas, registration control,
accounts, jobs, and security events, never content or content keys.

## 12. Distribution and legal boundary

- Server, web, and desktop: `AGPL-3.0-only`.
- Crypto core, protocol surface, and mobile: `Apache-2.0`.
- Documentation: `CC-BY-SA-4.0`.
- Kamori name and brand assets are reserved by the trademark policy.

The project currently has no legal entity. Legal texts and CLA are templates
and public registration must remain closed until an operator has completed
legal review. The public beta is 18+.

Planned artifacts are signed/notarized desktop packages for macOS and Windows,
Linux AppImage/deb plus a signed Flatpak repository, Android Play AAB,
Accrescent APKS and universal APK, and an iOS archive. Store submission and
production signing credentials are external release steps. Flathub is not an
MVP gate.

## 13. Release gates

The repository gate is Rust formatting/check/clippy/tests (including FRB), web,
admin and desktop checks/builds, Flutter analyze/tests plus real Android/iOS
native builds, container builds, and Pulumi Go formatting/vet/tests. Lockfiles
and generated FRB/WASM bindings are committed.

Before a public beta, operators must also execute and record the database
restore/failover exercise, backup verification, supported DAV-client matrix,
signed artifact smoke tests, mobile system-projection tests on physical
devices, dependency/license review, and the legal checklist. No documentation
may claim an independent security audit until one has actually occurred.
