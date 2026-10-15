# Kamori E2EE lifecycle design — 2026-10-16

Design proposal only; no repository changes, secrets, service calls, or suites were run. Repository reviewed at `/Users/yujin/Development/kamori-app/kamori-app/kamori-app`, requested branch `fix/e2ee-trust-boundaries`. Scope: recoverable handling of stale/revoked offline writes and explicit recovery from account master-key compromise. This needs separate implementation stages and product review; it is not a one-handler password fix.

## Evidence and existing invariants

- `apps/web-frontend/src/lib/components/app/AppWorkspace.svelte:917`: flush sends the original queued envelopes and stops globally on the first error. Full sync, around 1300–1500, refreshes packages and pulls operations, filters inaccessible materialized state, then replays **every** queued envelope, reintroducing inaccessible-space data.
- `packages/crypto-core-lib/src/local_bridge_runner/mod.rs:370`: native sync flushes each space before pulling; either failure aborts the cycle. `:1833` sends unchanged envelopes and aborts that space on the first error.
- `apps/cloud-server/src/features/operations/services.rs:62` rejects stale epochs before duplicate lookup; `repositories.rs:89` also locks only the matching current epoch before duplicate lookup. An operation accepted before an epoch rotation, with its acknowledgement lost, is therefore indistinguishable to today's client from an unaccepted stale operation.
- `packages/crypto-core-lib/src/recovery.rs` encodes the master bytes directly as 24 BIP39 words. `crypto.rs:94` deterministically derives the recovery HPKE private key from that master. Web `cryptoVault.ts` derives the credential-reset verifier from the same root. A leaked master consequently grants content recovery and the existing account-reset capability.
- `auth/services/password.rs` intentionally rewraps the same master for ordinary password change. Account recovery resets authentication and revokes old devices, but retains the root/recovery identity. `devices/repositories.rs:83` revokes device/session access and deletes device packages without rotating content keys.
- Existing `spaces/services.rs`/`repositories.rs` already support owner-authorized, request-hash-bound epoch rotations with current cursor checks, full device/member recovery package coverage, metadata and conflict-preserving snapshot-v2 coverage. Reuse that mechanism instead of inventing another content log.

Normal password changes must continue to mean password/OPAQUE replacement plus wrapping the **same** master. They do not promise root-compromise recovery. Revocation cannot erase downloaded ciphertext, keys, plaintext, backups, screenshots, or offline devices.

## A. Durable outbox lifecycle

### Persistent record and state machine

Keep the original signed envelope immutable. Add an outbox lifecycle record scoped by backend/account/space/client operation ID, containing original-envelope digest, current send-envelope bytes/digest, source device and epoch, insertion order, typed status/reason, attempt metadata, optional acknowledged sequence, and optional explicit import mapping. Encryption at rest must match the existing vault/SQLCipher protection; do not introduce plaintext draft storage.

Statuses: `queued`, `checking_receipt`, `ready`, `retry_later`, `needs_epoch_key`, `needs_review`, `blocked_by_dependency`, `access_removed`, `device_revoked`, `acknowledged`, `superseded_by_import`, and `discarded_by_user`. These are durable transitions, not a replacement for incoming-envelope quarantine. Existing queued rows migrate to `queued` retaining exact bytes/order. Invalid legacy rows become visible `needs_review`, never disappear.

Use a shared, independently testable scheduler/state machine with thin web/native adapters where practical. Dependency information is authenticated after opening the envelope; persist it locally for scheduling with the same protection as the cache. An outgoing operation remains recoverable until an acknowledgement and corresponding materialized-state update are durable, or the user explicitly discards/exports it. Archive original envelopes for replacements/imports.

### Refresh, reconcile, then send

1. Refresh account generation, current device status, complete accessible-space listing, roles, key epochs, and packages before choosing any send. A network failure, pagination gap, or single-space metadata error is not evidence of revocation. A successful complete listing can classify absent spaces; otherwise keep prior status and retry.
2. Per accessible space, pull/verify/materialize current authoritative operations and snapshots, preserving conflict branches, and reconcile known queued IDs. Isolate errors by space. Missing keys/quarantined server streams block affected resources/dependency descendants while unrelated spaces continue.
3. Consult hash-bound receipts for pending IDs not found in the entitled pull range. A snapshot/current-state cursor can omit the old operation itself, so a normal delta is insufficient proof.
4. Classify each pending item against current membership, role, author device and epoch; schedule ready operations in causal/topological order with insertion order only as tie breaker. A failed parent holds its descendants. A space-local error need not stop independent resources in that space when dependencies are known; at minimum it must never stop unrelated spaces.
5. Send ready envelopes. Persist acknowledgement and materialized sequence before deleting the sendable queue row, using one transaction where the store permits it. Crash recovery must tolerate either observation order without duplicate materialization. Return structured per-space/per-operation results, not an early-return global success/failure.

Web full sync must overlay only locally pending edits that remain eligible in accessible spaces. Access-removed/read-only/revoked drafts go to a distinct recovery/review view, never back into ordinary active collections or DAV projections. Offline state must say access is not yet revalidated; an honest disconnected device cannot learn immediate revocation. Native DAV writes must not keep reporting successful cloud sync for blocked drafts; retain local changes and expose their exact state.

### Lost acknowledgement and idempotency

Add a small batched, authenticated receipt endpoint: request `(space_id, client_op_id, known_envelope_digest[])`; response `accepted_exact(digest, space_seq)`, `different_bytes`, `absent`, or `unavailable`. Canonical domain-separated envelope hashing includes every signed field and signature, independent of MessagePack map serialization. Bound batch size and avoid exposing records for arbitrary IDs.

Receipt reads require a live account session and current space read entitlement; they are independent of whether the original author device is still write-authorized and independent of the current key epoch. A newly enrolled device of the same account may reconcile an old local queue after explicit recovery. Current readers may obtain receipts for already committed writes. Revoked membership gets no new space metadata; keep its drafts `access_removed/acceptance_unknown`. A receipt never appends bytes, enrolls a device, reinstates membership, or makes a revoked signature usable for new writes. Do not relax append authorization to solve receipt lookup.

If exact original bytes were accepted, acknowledge them even if the epoch changed or the author was revoked later. No replacement write is created. A different digest is a durable explicit conflict, never treated as success. Existing log retention permits calculating receipts from stored envelopes; a future compaction design must retain receipt digests/sequences for idempotency.

For an **unaccepted stale-epoch operation from the still-active same author device**, open the old envelope, validate its operation and dependency graph, and reseal the *same semantic operation and same client operation ID* under the current epoch with a fresh nonce/signature. Keep dependency IDs unchanged. Persist replacement bytes before any network I/O; retries use precisely those bytes. Never reuse a nonce across different ciphertexts. This is transport resealing, not editing the PIM operation to adopt the newest remote head.

Only reseal after a confirmed receipt absence and an observed strictly later committed epoch. The server rejects the old-epoch original thereafter; space/operation locks and the unique ID constraint remain authoritative against concurrent writers. If an accepted old envelope is nevertheless discovered, resolve only an exact known-original digest to its receipt; conflicting bytes require review. If an epoch rotates again, repeat receipt reconciliation first, then persist another replacement. New-epoch reads do not automatically erase the old content key needed to recover drafts.

If existing DAG validation/snapshot coverage cannot resolve the old parent, retain `needs_review`; do not flatten the edit into an unconditional overwrite or change its parent to a current head. Multiple remote/local branches remain explicit conflicts.

### Revoked authors and explicit user actions

Never automatically re-sign an operation from a revoked device or silently migrate it into a new author. Quarantine those drafts. The user may review/export/discard them, or **explicitly import** them as new edits on a currently authorized device into an accessible writable destination. Import allocates new IDs and records the old-to-new mapping and provenance; it is a new user action, not a replay authorized by the old device.

Import a dependency closure in topological order, mapping local pending-parent IDs to imported IDs and preserving accepted-parent references only when the destination snapshot/history actually covers them. A changed destination/resource/parent requires a reviewable merge or explicit fresh creation. Show delete and conflict effects before approval. On access removal, do not offer automatic republish into that space; an allowed export or deliberate copy elsewhere is a separate user decision. Explicit discard includes descendant impact and retains enough audit metadata to avoid accidental resurrection. No draft is silently dropped.

Typed server error codes distinguish stale epoch, membership removed, read-only, author revoked, conflicting duplicate, quota, authentication refresh required, retryable network/server error and malformed data. Where today's API gives an ambiguous error, refresh relevant state before assigning a permanent reason. Authentication failure blocks outbound authenticated work until restored; it need not delete anything or stop local review.

## B. Root/recovery rotation and safe lost-device recovery

### Separate operations and assurance levels

Expose three different actions:

1. **Change password:** current rewrap behavior, independent of root changes.
2. **Lost device / secure account:** revoke selected devices promptly, rotate affected future space epochs, and offer root/recovery rotation. Since runtime devices currently possess the account master, suspected device compromise defaults to root compromise; mere session revocation is incomplete.
3. **Replace encryption and recovery credentials:** explicit account-root transition to fresh random master `M_new`, new recovery HPKE identity, new recovery authorization credential, new recovery kit and updated wrapped root. Never derive `M_new` from `M_old`, the old mnemonic, or a predictable counter. Do not distribute `M_new` encrypted to the compromised root.

A safe version-2 recovery kit contains the new 24-word content root **and an independent random recovery authorization secret**, with account ID/backend, format version and recovery generation/checksum. The server stores only a domain-separated verifier/hash of that authorization secret. Routine devices need the content root/recovery HPKE capability but must not persist the kit-only authorization secret. Therefore leaking the runtime master does not by itself recreate future credential-reset authority. Keep the existing recovery HPKE derivation from the new master if desired; separation of the authorization credential is the important change.

This is a format/API migration, not something the existing 24-word kit already provides. A stolen entire kit remains a stolen recovery credential. A root-only attacker can also decrypt all data keys previously wrapped to that root; changing verifier alone does not undo this.

### Authorization and honest limitations

Normal authenticated rotation requires a recent, one-use, action-scoped reauthentication bound to user, auth/recovery generation, device, operation ID and the canonical transition manifest. Require current OPAQUE password proof plus an already enrolled independent factor when configured (verified passkey/user verification or TOTP), or a specifically defined sufficiently strong passkey flow. A bearer access token, possession/signature of `M_old`, newly self-enrolled factor, or old mnemonic-derived verifier alone does not constitute independent proof against root compromise.

Kit-only lost-all-device recovery uses the independent v2 recovery authorization secret plus possession of the content root, with a short-lived, narrowly scoped recovery capability. It can atomically replace account authentication/root state and enroll exactly the requested replacement device. Factor removal/reset, if needed because all factors were lost, must be an explicit part of this recovery manifest; recommend re-enrolling a factor before normal use. Do not silently remove a factor in an ordinary authenticated rotation.

For existing legacy accounts, the old kit remains the current recovery authority until an authorized migration commits. Upgrade under recent current-password/passkey authentication, preferably an independent second factor. A legacy kit-only emergency reset is an explicit lower-assurance fallback: the server cannot distinguish the owner from an attacker holding the same root/verifier, so it cannot guarantee winning a race or reverse a completed account takeover. If an attacker already reset the password and removed every independent factor, there is no cryptographic ownership discriminator; do not promise a support bypass. Product must choose whether legacy kit-only reset stays available during migration, uses a delay with independently authenticated cancellation, or is retired after an upgrade window.

### Prepare and verify before irreversible commit

Add a durable, request-hash-bound root-rotation operation with an operation ID, expected auth generation, expected root/recovery generation, expected package/membership inventory version, signed/canonical manifest, staged encrypted payloads, status and exact retry result. Prepare does not publish the new root identity or revoke the old kit. Rate-limit and expire uncommitted stages.

On a trusted client:

1. Reauthenticate; reconcile existing outbox receipts and stage/quarantine unresolved drafts before changing identities. Promptly revoke an explicitly identified lost device with the existing revocation action if full preparation will take time; show that key rotation is still pending.
2. Generate `M_new`, an independent recovery authorization secret, new signing/HPKE keys for the replacement device when compromise is suspected, and the new kit. Obtain/derive the correct password wrapping key with OPAQUE; never upload plaintext roots/content keys.
3. Enumerate all currently entitled recovery-package rows and current content-key inventory, including historical entitled packages that are actually retained. Open each with `M_old` or a trusted device key. Rewrap the same key bytes to `M_new`'s recovery public key, bound to space/epoch/user/new recovery generation. Preserve historical entitlements; do not resurrect revoked memberships or manufacture keys/history that this client lacks.
4. Verify every new package locally by opening it with the new identity and comparing the recovered key. Verify the new password-wrapped master locally. Missing/corrupt entitled packages block this all-or-nothing inventory transition; list affected spaces and allow repair or an explicit separately designed abandonment path, not silent omission.
5. Download/print the complete versioned kit. Require reimport of the generated kit or a meaningful local challenge covering both root words and authorization secret before finalization; verify account/backend/generation/checksum and decrypt a test package. A download-button click is not proof of a usable kit. The server never receives mnemonic/root/authorization secret plaintext except the protocol's reset credential proof as designed; no telemetry or logs include it.
6. Persist a locally encrypted exact commit request and new-generation bootstrap before network I/O. Use a one-time recovery/reauth capability bound to that exact manifest. Retries must not mint new secrets or regenerate probabilistic ciphertext.

### Atomic server transition and concurrency

Commit transaction verifies the live authorization binding and generations, exact inventory coverage, current membership/epoch/recipient generations and request hash, then atomically:

- publishes the new encrypted master, versioned recovery public bundle and authorization verifier;
- updates every retained entitled account recovery package to the new recipient generation;
- increments root/recovery and auth generations;
- invalidates outstanding legacy/v2 recovery attempts, enrollment grants, reauth capabilities and refresh sessions from prior generations;
- revokes compromised/selected devices (default all prior devices for root compromise), deletes their live device packages, and atomically admits the explicitly bound fresh replacement device/bootstrap;
- records the immutable committed manifest/result and protected-space remediation state.

Commit either updates all these together or publishes none. Staging rows can be uploaded in chunks, but activation cannot be a sequence of partially visible package/identity updates. Retry after a lost response returns the committed outcome for the exact request hash, even when its original auth generation is now obsolete; it must not execute a new mutation or return unrelated live secrets. Expose a narrow status/retry capability with suitable retention. Any changed request with the same operation ID fails.

Introduce `users.recovery_generation` (separate from auth generation), a recovery-credential version, and recipient generation on package DTOs/storage. Every space create, invite preparation, content-key rotation, recovery package put and enrollment path must compare supplied recipient generation/public-key fingerprint with the **current** account identity. Reject stale bundles and force repackaging. Otherwise another owner can race this operation and install a package still decryptable by `M_old` after rotation. Existing clients must negotiate lifecycle capability/minimum version and cannot overwrite new-generation packages via legacy unversioned endpoints.

Respect the current space-before-account-before-device lock discipline. A multi-space transition must discover inventory, lock involved space rows in deterministic order, then affected accounts/security bindings and devices in deterministic order, and revalidate an inventory-generation CAS before commit. Membership/package mutations must participate in that versioning. Do not hold an account lock while later acquiring a space lock. This is a required lock-graph review with concurrent append, revoke, invite and recovery tests; merely adding one account transaction around existing helpers risks deadlock. Impose reviewed staging/inventory size limits rather than truncating coverage.

### Content keys: owned versus foreign spaces

Root/package rewrap changes recovery access, **not** content-key secrecy. For each owned space, schedule/reuse owner-authorized epoch rotation with a fresh random space key, updated encrypted metadata, full packages for remaining legitimate devices/members at their current recovery generations, and signed snapshot-v2 coverage preserving all conflict branches. Exclude revoked devices. Old operations/history remain retained and already exposed data remains exposed.

Prefer marking owned spaces as remediation-pending at root commit and temporarily rejecting new appends until their snapshot/epoch transition is committed, so a user is not told compromise recovery is finished while future writes still use a disclosed key. Pulling and draft creation can continue with clear pending status. This is per-space availability impact; surface failures and retry exact rotation manifests. Do not require a giant all-space content snapshot transaction for root-credential replacement.

For foreign-owned spaces the recovering member can atomically replace **its own** recovery wraps but cannot rotate the owner-controlled space key, alter another member's access, or claim future confidentiality has been restored. Produce a pending-owner-action record/request requiring owner review, then perform the normal owner rotation. Do not send notifications/messages without a separately authorized product action. If owner action never occurs, the UI keeps the space at reduced assurance or the user explicitly leaves it. Readers have the same limitation; becoming read-only does not remove historical knowledge.

Protection status is per account credential transition and per space: credential rotation complete, owned-space epoch rotation pending/complete, foreign owner action needed, inaccessible/corrupt package repair needed. Never collapse these into a single unqualified 'all data secure' success.

### Offline devices and cache migration

Default root-compromise recovery revokes all old devices and does not HPKE-wrap the new root to any of them. Returning devices receive a generation/device-revoked result, stop outbound sync, retain encrypted local drafts/cache, and require fresh authentication plus explicit enrollment/trust. Preserve original-author provenance; use the A-flow receipt reconciliation and explicit draft import, never automatic signing as the revoked identity.

For a separately reviewed non-compromise rotation that retains trusted devices, migration needs a device-specific new-generation bootstrap bound to device identity; never wrap `M_new` with `M_old`. A device that lacks a trusted bootstrap must reauthenticate and re-enroll. Do not permit an old cached master/public bundle to overwrite generation-advanced state. Preserve old keys only as needed to read local historical data/drafts, isolated from new-generation signing/enrollment authority. Re-encrypt/rekey local root-wrapped vault/cache secrets crash-safely, including desktop/mobile fixed SQLite paths, then securely discard obsolete runtime copies where feasible. Local backups remain potentially compromised.

## Feasible implementation slices and migrations

1. **Outbox containment:** durable outgoing statuses and error taxonomy, accessible-space replay filter, per-space isolation, native refresh/pull-before-send, dependency-aware blocked states and user-facing pending reasons. Keep stale/revoked writes intact; no automatic reauthoring. This fixes current global stalls/data resurfacing without promising full recovery.
2. **Receipt reconciliation and stale resealing:** hash-bound receipt API; transactional web/native durable replacement records; active-author-only reseal with unchanged semantic IDs/dependencies; explicit import review. Migrate browser storage and SQLite outbox without queue loss, regenerate WASM/FRB/mobile DTO surfaces where changed.
3. **Credential lifecycle groundwork:** account root/recovery generation and credential version, versioned public bundle and recovery package recipients, generation enforcement on every key distribution write, v2 kit generation/reimport verification and independent recovery authorization flow. Legacy migration policy must be decided before deployment.
4. **Atomic root rotation:** durable staged manifests/receipts, inventory CAS and lock discipline, scoped authorization, atomic wrapped-root/recovery-package/device changes, encrypted client resume and offline migration. Update authentication/recovery/enrollment DTOs as bytes-oriented MessagePack.
5. **Compromise remediation:** owned-space write gate and existing snapshot rotations, foreign-space pending action, progress/resume UI, targeted acceptance tests. Root recovery is not complete by shipping slices 1–2 or simply exposing the existing rotate-space endpoint.

Database changes need uniquely numbered SQLx migrations: root/recovery generation/version and package inventory version; recipient-generation constraints; root-rotation staging/result tables; remediation records; outgoing local lifecycle metadata. Receipt lookup can initially use retained operation rows without a new server receipt table. Extend recovery attempts with expected recovery generation/action/manifest binding. Backfill existing users/packages as legacy generation 1; never infer v2 assurance from the presence of an old 24-word kit. Update ADRs 0004/0005 and the consolidated specification when approved.

## Required targeted validation

- Web/native: inaccessible queued space cannot resurface; one blocked space does not stop another; downgrade to reader and revoked author retain visible drafts without sending; incomplete membership fetch does not imply removal.
- Lost acknowledgement accepted before rotation: exact receipt advances local acknowledgement with no second operation; stale absent operation reseals with same ID/dependencies and fresh nonce; changed bytes with same ID fail visibly. Concurrent append/rotation/reseal and repeated rotations cannot create duplicate logical branches or consume a second sequence for a retry.
- Parent/child pending chains, accepted parent omitted by snapshot cursor, missing parent, independent resources, remote concurrent branches, deletes and explicit import mapping preserve causal/conflict behavior. No silent branch flattening.
- Crash/restart at every queue replacement/ack/import and root prepare/commit boundary; exact retries with lost responses; changed-manifest rejection; legacy local migrations preserve every queued envelope and stable order.
- Rotation: old verifier/legacy kit/token/enrollment grant cannot authorize a new reset after commit; new kit can restore password/root/current packages; root-only compromise cannot derive v2 reset credential. Ordinary password change preserves root and recovery credential; cancellation before commit preserves the old kit.
- Atomicity fault injection between every write: no mixed root/package generation or partial device revocation becomes visible. Concurrent foreign-owner rotation/invite/recovery-package put with stale recipient bundle is rejected. Inventory changes fail CAS and require restaging.
- Account-security/space append/revoke/recovery races and lock-contention tests exercise the documented order. Current-membership authorization governs receipts, and receipt reads cannot append under a revoked author.
- Owned spaces exclude revoked devices, rotate fresh content keys, retain all snapshot conflict branches and block/remediate writes as designed; foreign spaces remain pending without unauthorized owner actions. Returning offline devices cannot resume old signing or overwrite new-generation packages.
- MessagePack round trips assert binary fields remain bytes. Run targeted Rust feature/unit/integration tests, web check/unit/build, FRB tests/codegen checks and mobile analyze/test for affected surfaces. No broad suites were executed for this read-only design.

## Decisions requiring product/security review

- Adopt a v2 kit with an independent offline authorization secret (recommended) versus retaining the weaker root-equals-reset-authority model; decide legacy kit-only reset transition/delay policy and exact independent-factor requirements.
- Confirm default revocation of all prior devices for suspected root compromise, how the user selects trusted replacement device/factors, and whether ordinary recovery resets versus retains existing factors.
- Approve temporary owned-space write blocking during remediation and explicit reduced-assurance foreign-space status; define owner-request UX separately from authorized notifications.
- Approve draft review/export/discard/import behavior, descendant handling and retention; determine how long original envelopes/receipts remain locally available.
- Define treatment of an irreparable/missing entitled package, staging/inventory limits and honest reporting if recovery succeeds for credentials but a space needs repair. Full repair cannot be claimed on partial coverage.
- Decide whether history remains retained under old content epochs (current policy) or a later, separately scoped archive migration is desired. No design can erase previously copied historical plaintext/keys.
