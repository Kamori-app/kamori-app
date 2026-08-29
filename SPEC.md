# Kamori MVP specification

Status: implementation-aligned contract
Last updated: 2026-08-28

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
- Passkeys are implemented on the trusted web origin. Desktop passkey sign-in
  uses an expiring external-browser device-authorization flow; the Tauri
  WebView never performs a WebAuthn ceremony for `kamori.app`. Native mobile
  passkey plumbing is explicitly deferred; mobile exposes password plus
  optional TOTP sign-in.
- TOTP is optional. Its one-time backup codes are distinct from the data
  recovery kit and are atomically consumed.
- Sensitive account changes consume a short-lived, single-scope OPAQUE
  reauthentication proof. When a password sign-in needs TOTP, the second
  factor completes the same one-time server-held continuation instead of
  repeating the password exchange.
- Access tokens are short-lived. Refresh tokens rotate, detect reuse, and can
  be listed and revoked. Web refresh transport uses HttpOnly cookies plus CSRF
  and Origin/Referer validation; native clients use body transport. Every
  client persists a random per-generation rotation request id before sending a
  refresh. An exact retry returns the same still-active replacement for its
  lifetime; a different reuse id revokes the account's refresh sessions. Web
  refresh also rotates its host-only CSRF cookie so a lost response cannot
  accidentally advance the replacement session again.
- Registration is disabled by default and additionally protected by a strict,
  operator-configurable active-account cap.

All active application endpoints use MessagePack with human-readable Serde
semantics for cross-language types. UUIDs are canonical lowercase hyphenated
strings; binary protocol values stay MessagePack `bin` values rather than JSON
number arrays.

## 3. Devices, keys, and recovery

Every device has an Ed25519 signing key and an X25519 HPKE key. Device private
keys stay in browser encrypted IndexedDB or the platform secret store. A
successful sign-in issues a five-minute enrollment grant that can bind to one
exact device-registration request; a changed request cannot reuse it. The
client then unwraps the caller's current account-recovery packages and uploads
the new device packages. Authentication alone never returns plaintext space
keys from the server.

The web client provides device listing, approval, naming metadata, and
revocation. A revoked device cannot append new operations. Historical signing
keys remain discoverable when needed to verify existing log entries.

At web registration Kamori creates a 24-word BIP39 data recovery kit. It
deterministically derives an account master key and a separate X25519 account-
recovery identity. The server receives only a domain-separated kit verifier,
the recovery public bundle, and HPKE-wrapped current space keys. The browser
may also generate a plaintext `kamori-recovery-<random>.txt` copy entirely
client-side. Neither that file nor its name is uploaded; the UI warns the user
to move it out of Downloads and keep it separately from the password and daily
device. Recovery:

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
UI. Only the space owner may issue one. Before issuing, the client completes a
current-state key rotation and binds the invite to that committed rotation.
The issuer chooses an expiry from 15 minutes through 7 days and a reader or
editor role. Codes are normalized, hashed, rate-limited, atomically redeemed,
and exactly idempotent for network retries. The recipient installs both a
device package and a recovery package and receives current state, not prior
epoch history.

Invite preparation and member removal require complete new-epoch packages for
every remaining active device and member recovery identity. The owner submits
a stable rotation id, the exact base sequence, signed encrypted snapshots for
every materialized stream, and explicit quarantined-stream identifiers. The
server validates full coverage and commits packages, snapshots, membership,
metadata, and epoch atomically. Exact retries return the committed epoch;
stale or different requests conflict. Removed parties keep anything they
previously decrypted; Kamori does not claim retroactive erasure. Ownership
transfer requires explicit offer and acceptance. Spaces enter a 30-day trash
before purge, and account deletion cannot orphan shared assets.

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
field-oriented PIM upsert/delete payloads. The stable operation envelope type
has zero or one semantic parent; its current field schema is version 2, while
schema-1 payloads remain readable. Snapshot v2 is a signed, encrypted full per-stream checkpoint
that preserves every explicit conflict branch. Epoch rotation requires one for
every materialized stream before old keys are superseded; an authenticated
stream that could not be decoded must instead be listed as quarantined so it
cannot permanently block revocation. Unsupported mandatory key-control
envelopes still stop sync. General background compaction remains deferred; the
hosted beta retains its operation log instead of promising a 90-day window.

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

Membership history entitlement and current-state recovery are separate
cursors. Clients never move a persisted cursor backwards. A device holding
only the current epoch starts immediately before that epoch's verified
snapshots, which makes cross-device recovery independent from old epoch keys
without granting a newly invited member historical access.

## 7. Current PIM scope

Web and mobile expose separate task, calendar, and contact workflows rather
than a raw resource list:

- tasks have open/completed views, due date and time, completion time,
  priority, notes, one relative DAV alarm, categories, search, and
  completion/reopen actions;
- events have month/week/agenda presentation, all-day or timezone-aware start
  and end values, location, notes, one relative DAV/system alarm, recurrence, and
  categories;
- contacts have display and structured names, multiple labeled email addresses,
  phone numbers, and postal addresses, organization, job title, birthday,
  website, notes, categories, favorite state, search, and deterministic sorting;
- all three support creation, editing, deletion, offline queueing, sharing, and
  visible conflict copies.
- the web client exposes current deleted branch heads in Trash. A restore is a
  new signed upsert whose semantic parent is the tombstone and whose encrypted
  payload carries the last lossless projection available to that device. A
  tombstone obtained without its earlier decryptable version is visible as
  unavailable and cannot be reconstructed by the service.

The version-2 PIM field schema has typed date, UTC-instant, and zoned local-time
records plus multi-value records. A shared Rust parser/materializer is the
canonical iCalendar/vCard boundary used by native clients and the DAV bridge.
First-party partial edits replace only fields they explicitly manage, preserve
unknown properties and recurrence exceptions, and replace only Kamori-marked
alarms. Imported custom RRULE values, labels, priorities, and reminder offsets
remain selectable and are not normalized away merely by opening the editor.

The current event UI authors common daily/weekly/monthly/yearly rules and one
relative alarm for compatible DAV clients or an enabled mobile system
projection. First-party background notification delivery is not implemented.
Task repetition is preserved when imported but is not
authored until occurrence/exception completion semantics are implemented.
Attendees and scheduling, an advanced recurrence/exception editor, multiple
alarms, contact photos/groups, semantic search, bulk import/export UX, and a
polished conflict resolver remain post-MVP.

The 24-word Data Recovery Kit can be copied, revealed, or downloaded as a
browser-local plaintext file both during registration and later from Security.
The file is never sent to the service and its randomized filename contains no
account identifier.

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
platform permission, and is one-way from Kamori in this MVP. Calendar projection
includes time/all-day state, location, description, recurrence, and reminder.
Contact projection includes structured names, labeled contact methods, postal
addresses, organization/title, website, and birthday; notes are omitted on iOS
because that field requires a separately approved entitlement. Tasks remain
inside Kamori. Disabling projection lets the user keep or remove projected
copies. Plaintext written to a system provider leaves Kamori's E2EE boundary.
An integration choice is persisted before plaintext projection begins; after
an interrupted projection the collection remains visibly enabled and is
reconciled on the next sync instead of being reported as disabled.

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
nodes, one PostgreSQL primary with continuous encrypted PITR, an ops node,
protected volumes, firewall, and load balancer. Backblaze B2 is the primary
ciphertext store and a private Hetzner Object Storage bucket is the independent
DR target. Deployment remains
an explicit operator action; CI does not provision production by default.

The repository includes container definitions, Caddy routing, Prometheus,
Grafana, Alertmanager, backup/restore scripts/runbooks, and
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
Accrescent APKS and universal APK. The signed iOS IPA is an explicitly enabled
channel and remains unavailable to users until its TestFlight and
physical-device gates pass. Store submission and production signing
credentials are external release steps. Flathub is not an MVP gate.

## 13. Release gates

The repository gate is Rust formatting/check/clippy/tests (including FRB), web,
admin and desktop checks/builds, Flutter analyze/tests plus real Android/iOS
native builds, container builds, and Pulumi Go formatting/vet/tests. Lockfiles
and generated FRB/WASM bindings are committed.

Before a public beta, operators must also execute and record the database PITR
restore exercise, backup verification, supported DAV-client matrix,
signed artifact smoke tests, mobile system-projection tests on physical
devices for every released mobile channel, dependency/license review, and the
legal checklist. Excluded channels must be explicitly recorded as not
applicable in the release evidence rather than silently omitted. No
documentation may claim an independent security audit until one has actually
occurred.
