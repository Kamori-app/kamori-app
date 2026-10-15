# Task 3 — DAV identity and independently packaged native HTTPS

Implemented C01/C05 in the authorized isolated worktree, starting from
`0425219c4e7c16eda2b39af7be7d407310aa91cf`. No production/network service, real
credential, SECRETS.md, private PKI, OS trust-store change, push, or subagent was
used. All test credentials, keys, SQLite databases, and TLS certificates are
synthetic. Systematic debugging, TDD, and verification-before-completion skills
were applied; review is a self-review as directed by the controller.

## Changes

- DAV PUT resolves the selected cached head by projection href and keeps its
  random or deterministic stream UUID, canonical logical href, and parent.
  Only a previously unknown href derives a v5 stream. Conflict hrefs continue
  their selected branch in the same original stream.
- DAV DELETE takes its actual PIM kind from the head/ancestry, retaining Task
  for VTODO tombstones. Task/event kind changes on an existing stream reject.
- A shared head-resolution helper checks that the head matches the projection
  and route, then validates that its complete ancestry exists, stays in the
  same stream/route, and has no cycle before any outbox/state mutation.
- Tombstoned href recreation follows the existing tombstone and recovers the
  PIM kind from its live ancestry. This edge case was caught during self-review
  and has its own HTTP regression.
- Both core and desktop reqwest declarations explicitly enable `rustls`.
  Core FRB authentication and the sync runtime use the same native TLS policy
  builder while retaining their existing timeout policies.
- Android uses an explicit rustls client configuration with the already locked
  Mozilla `webpki-roots` trust anchors. Other native platforms retain reqwest's
  platform verifier. Production certificate, signature, chain and hostname
  checks remain enabled.
- `signature_covers_every_client_field_and_ciphertext` now separately mutates
  space_id, stream_id, client_op_id, author_device_id, key_epoch, envelope_kind,
  cipher_suite, nonce, ciphertext, and signature, checking rejection each time.
- Added a normal-dependencies-only isolated feature gate, a real in-process TLS
  server with a synthetic CA, and public fixture identities. `tokio-rustls` and
  Tokio IO utilities are necessary test dependencies for that real server.
  Cargo.lock adds only three direct dependency edges to already locked packages;
  no package versions were upgraded.

## RED evidence

All Cargo commands used
`PATH=/Users/yujin/.cargo/bin:$PATH CARGO_TARGET_DIR=/private/tmp/kamori-audit-target-197 rustup run 1.97.1 cargo`.
Loopback/source operations ran with the required worktree tool escalation.
Detailed logs are under `/private/tmp/kamori-audit-20261015/`.

1. `test --offline -p crypto-core-lib --test dav_conformance preserves_first_party -- --nocapture`
   failed on the first-party Task PUT: HTTP **500**, expected **204**.
   Log: `task3-dav-red.log`.
2. `test --offline -p crypto-core-lib --test dav_conformance rejects_broken_heads -- --nocapture`
   failed because a rejected HTTP PUT added an encrypted operation to the
   outbox after its cached head state was removed.
   Log: `task3-head-red.log`.
3. `test --offline -p crypto-core-lib conditional_http_conflict -- --nocapture`
   failed on conflict-copy PUT: HTTP **500**, expected **204**.
   Log: `task3-conflict-red.log`.
4. `tree --offline -p crypto-core-lib --no-default-features --features frb -e features -i reqwest`
   showed only reqwest's `json` feature, with **no TLS backend**. This was run
   before either manifest fix. Log: `task3-tls-red.log`.
5. Self-review regression:
   `test --offline -p crypto-core-lib --test dav_conformance recreates_a_deleted -- --nocapture`
   failed on recreating a tombstoned first-party task: HTTP **500**, expected
   **201**. Log: `task3-recreation-red.log`. Head validation was then corrected.

The signature invariant extension did not need an artificial RED stage because
it strengthens assertions for existing correct cryptographic behavior.

## GREEN evidence

- `test --offline -p crypto-core-lib --features frb`: **62 unit tests + 8 DAV
  integration tests passed**, zero failed/ignored; doc tests also succeeded.
  Log: `task3-core-full.log`.
- `test --locked --offline -p crypto-core-lib --no-default-features --features frb --lib`:
  **51 passed**, zero failed/ignored, including both real production factory
  TLS regressions. Log: `task3-isolated-frb-tests.log`.
- `clippy --offline -p crypto-core-lib --features frb --all-targets -- -D warnings`:
  passed with zero warnings. Log: `task3-clippy.log`.
- `bash packages/crypto-core-lib/tests/check_native_tls.sh`: passed independently
  for core FRB and desktop normal dependency graphs; Android graph includes
  bundled Mozilla roots. The script excludes dev dependencies and never selects
  cloud-server. Log: `task3-isolated-tls.log`.
- `check --locked --offline -p crypto-core-lib --no-default-features --features frb --target aarch64-linux-android --lib`:
  passed with the installed NDK. Log: `task3-android-check.log`. Compiler settings:
  `CC_aarch64_linux_android` and `CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER` point
  to NDK 28.2.13676358's `aarch64-linux-android24-clang`;
  `AR_aarch64_linux_android` and `RANLIB_aarch64_linux_android` point to its
  `llvm-ar` and `llvm-ranlib`. No device/emulator execution was claimed.
- Desktop isolated library check and final formatting: see final verification
  addendum below.
- `git diff --check`: passed.

The three added DAV integration tests use authenticated real loopback HTTP with
ETags. They cover random-stream task/event/contact update and delete, stale
conditional retry without another queued envelope, missing head/cross-stream
ancestor/cycle rejection for both PUT and DELETE without an outbox change, and
recreation after a Task tombstone. The unit-level conflict test also uses real
HTTP; it checks the original stream/selected parent, immediate projection,
Task tombstone, unaffected surviving branch, and duplicate replay of the complete
history into another runner through the real operation application path.

The TLS suite performs genuine TLS handshakes using the actual shared, cloud,
and FRB mobile production builder paths. An untrusted synthetic CA is rejected;
a test-client-only CA succeeds with a 200 `secure` response; a wrong hostname
with that trusted CA is rejected. The Android connector policy also runs these
same success/negative checks on the host without a JVM. Tests never disable TLS
verification or mutate platform trust. The test leaf private key is intentionally
public fixture data and is documented as such; the temporary synthetic CA signing
key is not committed.

## Platform trust rationale and limits

Primary installed sources inspected:

- reqwest **0.13.4** `Cargo.toml` feature definitions (`rustls` enables
  rustls-platform-verifier; `default-tls` points to rustls).
- reqwest `src/async_impl/client.rs` Rustls branch, including platform-verifier
  construction and preconfigured TLS handling.
- rustls-platform-verifier **0.7.0** README's Android integration and crate
  initialization sections: Android needs the verifier's Java/AAR integration
  plus `init_hosted`/`init_external` with JVM and application Context before
  networking. The hosted Flutter FRB path does not establish those resources.

Merely enabling reqwest `rustls` would therefore leave Android exposed to missing
verifier initialization. The controller approved bundled Mozilla public roots
on Android, platform verification elsewhere. The Android configuration selects
an explicit aws-lc crypto provider and safe TLS protocol versions; it has no
JNI, Android application Context, or trust-store mutation dependency. Direct
Android FRB compilation and host execution of that connector verify this path.

Android trusts the bundled public root set only: user/private CA additions and
Android network-security-config custom trust rules are not implemented. Root
updates ship with client releases. Desktop/iOS use platform trust; this task did
not execute Windows/Linux/iOS device TLS handshakes or packaged Android/Flutter
startup. Synthetic CA/hostname checks on the host and Android cross-compilation
are deliberately narrower evidence. Full native packaging/device release gates
remain with the controller after all tasks.

Replica replay exercises the real decrypted operation application path without a
cloud service. No production HTTPS request or real server upload/download cycle
was used. Queue, operation-state, and projection writes remain separate existing
transactions; this task does not claim power-loss atomicity across those writes.

## Self-review and intermediate failures

- Reviewed head lookup, canonical/conflict href handling, tombstone kind/ancestry,
  conditional retries, locking, and every queue call ordering in the changed DAV
  methods. Validation completes under the existing materialization lock before
  queuing. Existing public runner/DAV interfaces and schema are retained.
- A newly added event fixture initially omitted `starts_at`, then used a text
  value instead of the required temporal record; the contact fixture initially
  used `display_name` instead of required `title`. These were test setup failures
  after the initial real Task RED, corrected to match actual public PIM schemas.
  Final suite passes every resource-kind branch.
- Clippy initially rejected the mutation array's complex type; a small local
  alias fixed it. No lint suppression was added.
- The first Android check lacked the NDK ranlib override and OpenSSL's build
  reported `aarch64-linux-android-ranlib: command not found`. Adding the explicit
  NDK `RANLIB_aarch64_linux_android` made the same source graph compile.
- Self-review found the tombstoned href recreation regression before commit;
  a real HTTP RED test and ancestry-kind fix were added, then the full affected
  suite and Clippy rerun successfully.
- No unresolved source failures or deferred task requirements were identified.
  External DAV-client interoperability, cross-platform packaging, device runtime
  TLS, and broader workspace verification are controller/release gates.

## Changed files

- `packages/crypto-core-lib/src/local_bridge_runner/mod.rs`
- `packages/crypto-core-lib/src/local_bridge_runner/cloud.rs`
- `packages/crypto-core-lib/src/frb_api/transport.rs`
- `packages/crypto-core-lib/src/native_http.rs`
- `packages/crypto-core-lib/src/lib.rs`
- `packages/crypto-core-lib/src/operation_envelope.rs`
- `packages/crypto-core-lib/tests/dav_conformance.rs`
- `packages/crypto-core-lib/tests/check_native_tls.sh`
- `packages/crypto-core-lib/tests/fixtures/tls/{.gitignore,README.md,ca.pem,ca.der,server.der,server-key.der}`
- `packages/crypto-core-lib/Cargo.toml`
- `apps/dav-bridge-desktop/src-tauri/Cargo.toml`
- `Cargo.lock`
- This task report.

## Final verification addendum

- `check --locked --offline -p dav-bridge-desktop --lib`: passed independently
  on aarch64-apple-darwin. Log: `task3-desktop-check.log`.
- `fmt --all -- --check`: passed, empty diagnostics.
  Log: `task3-format.log`.
- Final isolated production TLS script: passed all three checks.
- Final full core run: **62 + 8 tests passed**; isolated FRB run: **51 passed**;
  final strict Clippy: passed; Android isolated FRB check: passed.
- Final `git diff --check`: passed.

## Review fix round 1 — validate every reconciliation branch before persistence

Review: `/private/tmp/kamori-audit-20261015/task-3-review.md`, Important finding 1.
Base: `6470410c80793a6d27eee6453a75962d7d78c78a`.

The review identified a real gap: selected-head ancestry validation accepted a
healthy branch while a separate sibling in the same stream contained an absent
parent, cycle, or a projection seed on a nonroot node. Reconciliation validates
the complete stream after persistence, so this produced an HTTP error after an
outbox/state mutation. This corrects the earlier report's claim that no unresolved
malformed-graph rejection paths remained.

Change: extracted reconciliation's existing SQL graph read and
`assign_pim_branches` call into `load_validated_pim_stream`. Reconciliation and
DAV preflight both use that exact helper. Both PUT and DELETE call the preflight
before operation construction/queue/state persistence while still holding the
existing materialization lock. PUT's new-href path also preflights its resolved
stream. Selected-head route, stream and kind checks remain in place. No graph
validation rules were duplicated, no cache schema changed, and no TLS source,
dependency declaration, or lockfile changed in this fix round.

Added six independently runnable real authenticated HTTP regressions:

- `rejects_unselected_sibling_missing_parent_on_put_without_persistence`
- `rejects_unselected_sibling_missing_parent_on_delete_without_persistence`
- `rejects_unselected_sibling_cycle_on_put_without_persistence`
- `rejects_unselected_sibling_cycle_on_delete_without_persistence`
- `rejects_unselected_sibling_seed_misuse_on_put_without_persistence`
- `rejects_unselected_sibling_seed_misuse_on_delete_without_persistence`

Each creates a healthy selected Task branch and another materialized sibling,
corrupts only that sibling using the synthetic SQLite fixture, GETs the selected
resource/ETag, and issues a conditional PUT or DELETE. After the HTTP rejection,
it checks exact row equality for `operation_outbox`, `operation_states`,
`resource_heads`, `calendars`, and `dav_changes`, and confirms both HTTP projections
remain readable with the selected payload unchanged. The selected ancestry stays
valid in all six cases, which distinguishes them from the original head tests.

Commands below use the original pinned Cargo prefix and target directory stated
above; all worktree mutations and loopback tests used required tool escalation.

RED:

`test --locked --offline -p crypto-core-lib --test dav_conformance rejects_unselected -- --nocapture`

All **six failed** for the expected persistence violation: each rejected HTTP
PUT/DELETE changed the outbox count from **2 to 3**. Log:
`/private/tmp/kamori-audit-20261015/task3-fixround1-red.log`.

Focused GREEN after shared validation:

`test --locked --offline -p crypto-core-lib --test dav_conformance rejects_unselected -- --nocapture`

**6 passed**, zero failed/ignored. Log:
`/private/tmp/kamori-audit-20261015/task3-fixround1-green.log`.

Final verification after moving preflight onto both complete DAV write paths:

- `test --locked --offline -p crypto-core-lib --features frb`: **62 unit + 14 DAV
  tests passed**, zero failed/ignored; doc tests also passed. Log:
  `/private/tmp/kamori-audit-20261015/task3-fixround1-core-full.log`.
- `clippy --locked --offline -p crypto-core-lib --features frb --all-targets -- -D warnings`:
  passed. Log: `/private/tmp/kamori-audit-20261015/task3-fixround1-clippy.log`.
- `fmt --all -- --check`: passed, no diagnostics. Log:
  `/private/tmp/kamori-audit-20261015/task3-fixround1-format.log`.
- `git diff --check`: passed.

Self-review compared the preflight and reconciliation helper arguments and
confirmed identical collection, stream, and canonical projection ids. The
existing lock spans preflight and the write/reconciliation sequence, so ordinary
runner mutations cannot change the graph between them. Existing transaction and
power-loss limits still apply; this change closes graph-validation rejection
before persistence, not arbitrary I/O-failure rollback. TLS was unchanged, so no
duplicate Android/desktop build gates were run. The full core run still executes
the real shared/cloud/FRB/Android connector TLS tests. No remaining scoped review
finding is known; controller review remains the next gate.

Changed files in this fix round: `src/local_bridge_runner/cache.rs`,
`src/local_bridge_runner/mod.rs`, `tests/dav_conformance.rs` (all under
`packages/crypto-core-lib/`), and this report.
