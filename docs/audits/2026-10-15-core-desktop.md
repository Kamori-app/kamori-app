# Core/native runtime/desktop audit

Revision: `81248927f1ad734c246f582486d9840da6b47637` (2026-10-15).
Read-only review; no repository source edits, commits, secrets, credentials, or external services were accessed. Temporary synthetic audit resources are under this report directory. AGENTS.md and the complete SPEC.md were read.

## Findings

### C01 — P1 — DAV edits derive a new stream instead of retaining the existing branch stream

**Locations:** `packages/crypto-core-lib/src/local_bridge_runner/mod.rs:1458` and `:1563`; affected queue/reconcile sequence at `:1491–1513`; stream-scoped state selection at `cache.rs:768`.

PUT and DELETE compute `Uuid::new_v5(space_id, resource_href)` unconditionally. First-party web/mobile resources instead have a random stream UUID and a `<stream UUID>.ics/.vcf` href; conflict copies also have distinct hrefs for the same stream. The DAV command loads the correct parent operation by href but stores its child in the newly derived stream. Reconciliation selects only the new stream and cannot find that parent. It fails with `PIM operation dependency is missing`, after already persisting the envelope in the outbox and operation_states. A successful conditional DAV request returns HTTP 500 and leaves the original resource unchanged. Background flushing can upload the malformed cross-stream dependency, causing quarantine elsewhere. All first-party DAV update/delete and conflict-copy edit paths are affected.

**Executed reproduction:** `core-repro.rs` creates a first-party task using `LocalBridgeRunner::upsert_pim_item`, starts loopback DAV, GETs its href/ETag and PUTs a changed summary with If-Match. Output: `500 Internal Server Error internal local DAV error`; cache still contains `SUMMARY:Original`. The configured cloud endpoint is closed loopback port 9, so no external service is involved.

**Minimal fix:** For an existing head, load its cached state and retain its stream_id and actual PIM kind; derive a v5 id only for a genuinely new DAV resource. Validate that dependencies belong to that stream before queueing. DELETE currently also hardcodes CalendarEvent for VTODO tombstones; retain actual task kind from the existing projection/head.

**Regression tests:** DAV conditional PUT and DELETE of first-party event/task/contact, plus PUT/DELETE of an explicit conflict copy. Assert correct same-stream parent, immediate projection change, retry behavior, sync to another runtime, and no quarantined graph errors. A rejected write must not leave an invalid queued operation.

### C02 — P1 — Desktop bridge reconciliation decodes cloud UUIDs in the wrong MessagePack mode

**Location:** `apps/dav-bridge-desktop/src-tauri/src/commands/auth.rs:219`.

The runner branch of authenticated_get_msgpack calls `rmp_serde::from_slice`, whose deserializer is non-human-readable. Cloud endpoints deliberately serialize UUIDs as canonical strings. ListSpacesResponse / DeviceKeyPackage / RecoverySpaceKeyPackage contain Uuid fields; uuid 1.24's non-human-readable deserializer accepts only 16 raw bytes. Direct login uses the correct with_human_readable decoder, but each manual/background sync first calls provision_device_and_spaces through the runner and fails while decoding a nonempty /spaces response. Thus the bridge can start and serve stale/offline data while all normal cloud pulls fail.

**Executed reproduction:** encode `{space_id: Uuid}` using the repository's cloud serializer settings, then use the desktop runner decoder. Output: `invalid type: string "00000000-0000-0000-0000-000000000001", expected a 16 byte array`. The exact same bytes decode successfully with with_human_readable.

**Minimal fix:** Reuse a shared bytes decoder that constructs `rmp_serde::Deserializer::new(bytes).with_human_readable()` for the runner branch.

**Regression test:** Exercise authenticated_get_msgpack with a runner/fake server returning a realistic nonempty ListSpacesResponse containing UUID strings and byte packages. Include both direct and runner transport paths and a sync reconciliation cycle.

### C03 — P1 — Desktop-created spaces use an incompatible recovery-key package

**Location:** `apps/dav-bridge-desktop/src-tauri/src/commands/collections.rs:78–79`. Consumer: `commands/auth.rs:326–336`; reference implementation: `packages/crypto-core-lib/src/frb_api/devices.rs:58–76`.

Desktop create_collection constructs encrypted_recovery_key_package with `secret_vault::encrypt(master_key, space_key)` (EncryptedPayload: algorithm/nonce/ciphertext). All recovery/new-device consumers require EncryptedGroupKey (version/encapsulated_key/ciphertext), HPKE-encrypted to the account-recovery public identity. The server stores opaque bytes and cannot repair this. A fresh approved device or account recovery cannot unwrap a desktop-created space until another already-keyed device replaces the bad package. The creating desktop's regular reconciliation would normally rewrite it, but C02 currently prevents that path as soon as /spaces is nonempty.

**Executed reproduction:** Pass desktop-created package bytes to the actual required EncryptedGroupKey decoder. Output: `missing field version`.

**Minimal fix:** Derive the account recovery identity and HPKE-wrap the space key, matching the mobile/web protocol. Prefer a shared public recovery wrapping helper so clients cannot diverge. Consider migration/repair of already-created desktop packages while their original devices still hold the key.

**Regression tests:** Create space using desktop package construction, import on a second device with only the account master key, and verify recovery-kit-derived identity decrypts the exact key. Assert the package schema, not merely that opaque bytes are nonempty.

### C04 — P1 — Desktop startup bypasses durable runner refresh recovery and can revoke every account session

**Locations:** `apps/dav-bridge-desktop/src-tauri/src/commands/auth.rs:453–470`; independently generated keychain attempt `commands/common.rs:419–421`; runner attempt `packages/crypto-core-lib/src/local_bridge_runner/cache.rs:357–392`; runner recovery `local_bridge_runner/mod.rs:105–109`.

The keychain stores token T0 with random attempt K. The runner separately persists a random attempt R for T0 in SQLCipher before sending a refresh. If the server consumes (T0,R) and the process crashes before sync_runtime_tokens updates the keychain (including a lost HTTP response), startup restore_session sends (T0,K) directly with reqwest before opening the runtime cache. K differs from R. The server treats this as token reuse and revokes **all** account refresh sessions (`apps/cloud-server/src/features/auth/repositories/refresh_tokens.rs:249–310`). If the runner received the response and cached T1, the same bug occurs because restore_session ignores that committed replacement. The durable cache mechanism is correct in isolation but startup bypasses it.

**Evidence level:** Verified complete source call path plus server's exact revocation branch; not an OS-keychain/process-crash runtime reproduction. C01–C03 were executed separately.

**Minimal fix:** Before any startup refresh, recover the cache's committed credential generation and pending attempt, and use one durable token manager/attempt source for both startup and active runtime. Preserve exact retry identity across all transports; do not generate a second id for the same token generation.

**Regression tests:** Kill/restart after (a) server rotation with dropped response, (b) durable replacement cache commit before keychain update, and (c) keychain update. Exact retries must retain the replacement session and unrelated account sessions; no refresh_token_reuse event should appear.

### C05 — P1 — Packaged native clients do not enable HTTPS support

**Locations:** `packages/crypto-core-lib/Cargo.toml:65`, `apps/dav-bridge-desktop/src-tauri/Cargo.toml:18`.

Both reqwest dependencies set default-features=false and enable only json. Neither isolated desktop nor isolated FRB/native graph enables a TLS feature. The default server is https://api.kamori.app and URL validation requires HTTPS for nonloopback origins. These independently packaged clients therefore cannot connect to production HTTPS. Full workspace checks hide the issue because cloud-server's object_store dependency enables reqwest rustls via Cargo feature unification. DAV tests only use plain HTTP loopback, so they also miss it.

**Executed verification:** `cargo tree --offline -p crypto-core-lib --no-default-features --features frb -e features -i reqwest` and `cargo tree --offline -p dav-bridge-desktop -e features -i reqwest` both show reqwest's sole enabled feature is json. Local reqwest Cargo.toml defines default-tls -> rustls; local non-TLS connector delegates to plain Hyper HTTP connector, whose default enforce_http rejects HTTPS. No standalone production HTTPS request was executed.

**Minimal fix:** Explicitly enable a supported reqwest TLS backend in core/native and desktop dependency declarations. Validate platform trust behavior for Android/iOS/macOS/Windows/Linux without relying on workspace feature leakage.

**Regression tests:** Build/test native package graphs independently, then perform a local test-CA HTTPS request using the actual configured client. Keep a dependency feature assertion or equivalent isolated transport gate so cloud-server feature unification cannot mask missing TLS again.

## Inspected areas and test quality

- Envelope/crypto: crypto.rs, operation_envelope.rs, account_keys.rs, secret_vault.rs, recovery.rs, tests.rs. Signature bytes cover public identifiers, epoch, kind, suite, nonce and ciphertext; open authenticates AAD and stream HKDF context; device bootstrap binds the authorization flow. Existing envelope test named signature_covers_every_client_field_and_ciphertext only mutates ciphertext, so the name overstates its assertions. No additional primitive cryptographic flaw confirmed in this review.
- Native persistence/sync: all local_bridge_runner modules; cache schema, SQLCipher opening, plaintext legacy rejection, rotation credentials, cursor monotonicity, outbox order, graph materialization, quarantine, snapshot build/apply, per-stream DAV journal. Cache unit tests meaningfully assert high-sequence preservation, insertion ordering, atomic operation/head writes and journal tombstones. Queue, operation state and projection are still separate transactions at the orchestration layer; crash-injection end-to-end recovery is not covered by those atomic-unit tests. This is a gap, not an additional confirmed finding.
- PIM: validation, typed temporal fields, lossless patch materialization, managed alarms, unknown properties, recurrence exceptions, branch assignment and snapshot schema; inspected accompanying tests. The tests exercise specific preservation examples rather than complete imported corpus/property-based losslessness.
- Desktop: Tauri state/config/auth/session/collection/bridge command paths and UI transport/store/page contracts. Keyring-backed device/master/DAV/cache secrets are scoped with the expected purpose labels; DAV credential is random and separate from the account password. No live platform-keyring or packaged Tauri application smoke test performed.
- DAV: dispatcher, authenticated discovery, report parser, conditional writes, loopback validation, journal token/tombstone response paths; all five tests in tests/dav_conformance.rs read. They run real loopback HTTP against a real local runner but intentionally use closed cloud port 9. Consequently they do not verify uploading, cross-device sync, first-party-to-DAV editing, key rotation, or external DAV-client interoperability.
- Desktop frontend package.json defines test, lint, and check as the same svelte-check invocation. That is static analysis, **not runtime UI tests**. No desktop UI runtime test harness is present in the scoped files.
- No runtime skip-on-missing-service branches were found in scoped core/DAV tests. DAV conformance tests are entirely compile-gated by local-bridge and http-reqwest; feature-disabled builds execute zero of them. Scoped FRB modules contain only the crypto_exports unit-test section; auth/invite/runtime lifecycle integration coverage is sparse.

## SPEC-to-tests matrix

| SPEC contract | Existing relevant tests read | Evidence/remaining gap |
|---|---|---|
| §2 MessagePack UUID strings / binary fields | field_operation_msgpack_roundtrip_is_stable; desktop credential helper tests | Internal payload roundtrips do not cover cloud-to-runner decoder; C02 |
| §2 refresh exact retry after lost response | rotated_credentials_recover_only_from_the_current_or_previous_token; refresh_rotation_attempt_is_random_stable_and_token_scoped | Individual cache units pass by design; startup/keychain orchestration gap C04 |
| §3 device/recovery key packages | group_key_wrap_unwrap; account_recovery_identity_is_deterministic_and_unwraps_space_keys; device_bootstrap_is_bound_to_recipient_and_authorization_flow | Desktop creator does not use tested package format; C03 |
| §5 signed bound operation envelope | signature_covers_every_client_field_and_ciphertext; sealed_envelope_authenticates_public_context | Existing test mutations cover ciphertext and stream respectively; no exhaustive field-mutation suite |
| §5/6 snapshots, cursors, conflicts | snapshot_preserves_all_conflict_branches; branch_assignment_is_arrival_order_independent; advancing_sync_cursor_never_moves_it_backwards; acknowledgement monotonicity | Standalone data/model tests; restart + epoch rotation + previous cache histories not fully covered |
| §6 local writes materialized before upload | first_party_write_is_materialized_and_queued_offline; outbox_preserves_insertion_order_when_timestamps_match | Real offline cache path tested; no power-loss injection across queue/state/projection commits |
| §7 lossless PIM edits | contact_edit_preserves_parameters_groups_folding_and_unknown_properties; event_edit_changes_master_without_overwriting_recurrence_exception; rich event/contact/task tests | Meaningful field examples; imported corpus and advanced malformed timezone/recurrence coverage remain limited |
| §8 loopback dedicated auth / discovery / preconditions / tombstones | five real-HTTP dav_conformance tests plus dav parser tests | Good loopback/offline protocol coverage; misses first-party href/stream and conflict-edit transition C01; external supported-client release matrix remains an operator gate |
| §11/13 real hosted native transport | No isolated production HTTPS smoke gate found | Workspace feature union hides C05; Flutter mocks/typechecks do not prove native HTTPS |

## Commands and outcomes

- Read AGENTS.md, SPEC.md and scoped files via rg/sed/nl; `git rev-parse HEAD` -> revision above; `git status --short` -> clean during audit.
- Controller owns full Cargo baseline in `/private/tmp/kamori-audit-target`; this worker did not start a competing Cargo build/test sweep.
- `rustc --edition=2024 core-repro.rs -L dependency=/private/tmp/kamori-audit-target/debug/deps --extern ... -o core-repro` -> built small reproduction using baseline libraries. A first attempt linked the host Tokio rlib and was corrected to the target Tokio rlib; no source finding arose from that harness issue.
- Initial reproduction loopback bind was denied by sandbox; same synthetic loopback-only binary succeeded after tool escalation. Final outputs captured above for C01–C03.
- Both isolated cargo-tree commands for C05 succeeded offline. No external requests, full builds or source changes were necessary.

## Limits

This was a focused source/test audit, not independent cryptographic certification or execution of the public-beta release matrix. Browser/mobile UI and cloud authorization are assigned to other audit workers. Current-state snapshots and cache/account transitions deserve further restart/fault-injection coverage, but unproven concerns are intentionally excluded from the finding list.
