# Task 2 — sharing retries, operation accounting and append concurrency

Status: implemented and locally verified in the isolated worktree. Base: `a7397e8b62efb23c3d9aedc28281d07435b02fcc`.

## Result and implementation

S01: exact redemption identity remains the normalized invite hash plus authenticated recipient, without wire changes. The first redemption atomically records its effective response role, history floor and current-state floor in migration 002's new nullable columns. The ciphertext, note, space and epoch remain the immutable invite values. Retries return the original response without updating membership or used_count, including after the unused invite admission deadline. They still require the active space/current epoch, unrevoked invite and active matching-epoch entitlement with sufficient role/history. A stronger editor/owner entitlement remains untouched. Removed members, stale epochs, inactive spaces, revoked invites, expired unused codes and other recipients fail closed.

S02: migration `202610150002_release_purged_operation_usage.sql` adds a security-space BEFORE DELETE trigger that releases OLD.operation_bytes from its owner. The existing operation AFTER DELETE trigger continues to charge/release direct operation changes; cascaded children no longer see their deleted parent, so the new parent trigger releases the aggregate exactly once. The migration locks accounting tables and recalculates all space/account totals from live operations, including zero for owners with no remaining spaces. Committed old migrations are unchanged.

S03: append now locks the active/current-epoch space FOR UPDATE before account, membership or device rows. Owner quota uses FOR NO KEY UPDATE before the membership/device SHARE locks, preserving space-before-account and user-before-device ordering against Task 1 authentication/recovery/revocation. Same-client-operation advisory serialization, authorization-sensitive duplicate handling and final epoch/device/member checks remain intact.

## RED evidence

Tests were written before the corresponding fixes. Commands used the pinned Rust toolchain and disposable DB:

```sh
PATH=/Users/yujin/.cargo/bin:$PATH \
CARGO_TARGET_DIR=/private/tmp/kamori-audit-target-197 \
KAMORI_DATABASE_URL=postgres://kamori:kamori-audit-local@127.0.0.1:55439/kamori_audit \
rustup run 1.97.1 cargo test -p cloud-server features:: -- --nocapture
```

`/private/tmp/task2-red.log`: **93 passed, 4 failed**, after correcting initial fixture cleanup mistakes. Genuine business failures:

- `lost_redemption_response_retries_preserve_original_response_and_entitlement`: `expected successful exact invite redemption` on the consumed same-recipient retry.
- `concurrent_same_recipient_redemptions_return_identical_response`: same failure for the second concurrent request.
- `deleting_operations_spaces_and_workspaces_releases_only_their_charge`: expected `(282, 282)` account/live-operation totals, received `(564, 282)` after deleting one space while another remains.
- `distinct_concurrent_appends_allocate_sequences_without_lock_upgrade_deadlock`: `right distinct append must not deadlock: error returned from database: deadlock detected ...`. The test holds an external SHARE space lock, waits until both real append connections are visibly waiting on PostgreSQL locks, then releases the gate. Old SHARE-to-UPDATE upgrades deterministically conflict; no production test hook or mocked success is used.

Initial `/private/tmp/task2-invites-red.log` also contained fixture errors from deleting users before their RESTRICT-owned spaces. Those were corrected by deleting synthetic workspaces first and are not counted as business RED evidence. An inactive-space fixture was corrected to the actual `deleted` status before the authoritative RED run.

Final self-review added a real ownership-transfer case before adjusting the retry entitlement check:

```sh
# Same environment as above
rustup run 1.97.1 cargo test -p cloud-server redemption_retry_preserves_entitlement_after_recipient_becomes_owner
```

`/private/tmp/task2-owner-red.log`: **0 passed, 1 failed**, `expected successful exact invite redemption`. An already redeemed recipient promoted through the real ownership-offer acceptance flow was incorrectly excluded by the initial retry check. The final check accepts that stronger role while replaying the original response and retaining owner membership.

## GREEN and final verification

```sh
PATH=/Users/yujin/.cargo/bin:$PATH \
CARGO_TARGET_DIR=/private/tmp/kamori-audit-target-197 \
KAMORI_DATABASE_URL=postgres://kamori:kamori-audit-local@127.0.0.1:55439/kamori_audit \
rustup run 1.97.1 cargo test -p cloud-server

PATH=/Users/yujin/.cargo/bin:$PATH rustup run 1.97.1 cargo fmt --all -- --check

PATH=/Users/yujin/.cargo/bin:$PATH \
CARGO_TARGET_DIR=/private/tmp/kamori-audit-target-197 \
rustup run 1.97.1 cargo clippy -p cloud-server --all-targets -- -D warnings
```

Final server result: **141 passed, 0 failed, 0 ignored** with the DB configured. Final formatting and strict Clippy pass. Full output: `/private/tmp/task2-final-tests.log`, `/private/tmp/task2-final-clippy.log`.

Coverage includes:

- Exact first/retry ciphertext, note, role, epoch, history and current-state floor equality; unchanged membership and single use; existing editor entitlement; valid stronger owner entitlement after real transfer.
- Concurrent same-recipient responses and different-recipient single-use admission; expiry, member removal, epoch advance, inactive space and explicit invite revocation.
- Direct operation DELETE, space DELETE and workspace cascade; live account totals and another space's remaining charge; blob quota counters alongside operation counters.
- Real SQLx-ledger migration from a synthetic schema with **003 already applied before 002**: live-account counter drift and the old cascade leak on an otherwise empty account are repaired, including a deliberately drifted space counter. A separate empty schema verifies fresh sorted installation through SQLx. Both schemas are dropped after passing.
- Deterministic distinct signed-client-operation appends both accepted at sequences `[1, 2]`, next_sequence `2`, and exact account/space/live-operation bytes. The pre-existing concurrent same-id retry test is retained. Signed fixture envelopes are re-signed after client-id/space changes.
- Duplicate append rejects revoked device, revoked member and stale epoch.

## Browser sharing acceptance

The acceptance project's three public ports were confirmed free before starting. Only `COMPOSE_PROJECT_NAME=kamori-audit-20261015` was used. Cached unchanged web/admin images were reused and the cloud image was rebuilt from the fixed sources. No other test containers were stopped or reset.

The machine's derived Podman socket was stale on the first Compose build attempt; using its live synthetic local socket resolved this:

```sh
DOCKER_HOST=unix:///private/tmp/podman/podman-machine-default-api.sock \
COMPOSE_PROJECT_NAME=kamori-audit-20261015 \
/usr/local/bin/docker-compose --project-directory tests/acceptance \
-f tests/acceptance/compose.yaml build cloud

DOCKER_HOST=unix:///private/tmp/podman/podman-machine-default-api.sock \
COMPOSE_PROJECT_NAME=kamori-audit-20261015 \
KAMORI_CONTAINER_ENGINE=docker KAMORI_ACCEPTANCE_SKIP_BUILD=true \
bash scripts/acceptance.sh up

PATH=/Users/yujin/.bun/install/cache/@oven/bun-darwin-aarch64@1.3.14@@@1/bin:$PATH \
bun run --filter acceptance-tests full sharing.spec.ts
```

The sharing scenario passes with the new same-recipient retry assertion (including decrypted note), editor writes and reader write denial. Playwright emits its pre-existing NO_COLOR/FORCE_COLOR warning. Final rerun against cloud image `a3a8a233a957` after its rebuild/restart: **1 passed (26.4s), exit 0**. The earlier run also passed (27.3s). Logs: `/private/tmp/task2-sharing-acceptance-final.log`, `/private/tmp/task2-acceptance-up-final.log`, `/private/tmp/task2-acceptance-build-final.log`.

## Self-review and limits

- Pre-migration **already consumed** invites deliberately remain non-replayable: their exact effective original role/history/floor was never recorded and cannot safely be reconstructed after entitlement changes. Their existing membership is not modified. New redemptions have the durable response metadata required for safe exact replay.
- Migration repair scans/locks the operation accounting tables; plan deployment accordingly. No production action was taken.
- The PostgreSQL integration harness still follows the repository's existing optional-DB convention, but every server run reported here explicitly configured the disposable DB; no DB test was counted from a skipped local run.
- Only the targeted sharing browser scenario was run for this task. Broader acceptance/workspace verification remains with the controller's final integration run.
- Synthetic acceptance project is left running for the controller's final suite. No push, production access, real credential changes or private secrets reads occurred.
