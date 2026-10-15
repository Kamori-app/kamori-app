# Disposable local recovery exercise

Run from the repository root with Python 3, `/opt/podman/bin/podman`,
and the socket named in `run.py`:

```sh
python3 tests/release-readiness/restore/run.py
```

The default migrator is the exact local cloud-server image used for the
2026-10-15 exercise. On a later checkout, build the cloud-server acceptance
image first, then set `RESTORE_MIGRATOR_IMAGE` to its immutable local image ID.
Every successful SQLx migration checksum must match the current source files.
Every engine command passes Podman `--url` with the reserved Unix socket.
Inherited Podman/Docker endpoint selectors are stripped, and `RESTORE_ENGINE`
values other than this exact backend are rejected. The socket, backend, resource
namespace, labels and artifact root are deliberately fixed; changing their
ownership contract requires review. Containers have no host-published ports.

The runner builds only its Dockerfile context, applies real SQLx migrations,
seeds synthetic byte canaries, runs an encrypted pgBackRest full backup and the
unchanged production backup/archive-check job, and restores between committed
markers into a separate named PGDATA volume. It compares schema, every migration
SHA-384, operation bytes, cursors, owner counters and blob metadata, then runs
PostgreSQL's offline page checksum verification. Repository encryption uses
AES-256-CBC and a runtime-generated disposable passphrase; an incorrect key must
fail to decode backup information. The repository is local POSIX storage, so
this does not exercise pgBackRest's remote S3/TLS/retention configuration.

Three AES-CTR synthetic objects (128, 4,096 and 65,536 bytes) are copied by the
unchanged production rclone job between independent MinIO stores. A put-only
DR principal must fail; adding bucket-scoped list/get permissions must succeed.
The primary is then stopped and all DR objects are read against size/SHA-256
metadata restored from PostgreSQL. Removing an object and replacing an object
with same-length corrupt bytes must both fail integrity verification. Each
fault is repaired and the full DR-only read repeated. The production sample
size is three, covering every object in this bounded dataset.

Private artifacts are preserved under
`/private/tmp/kamori-readiness-20261015/restore/<UTC-run-id>/`: commands and exit
codes, image IDs/digests, installed package versions, SQLx/migration evidence,
expected/restored data and schema, recovery timestamp/server logs, ciphertext
manifest/restored files, failure injections, timings and cleanup inventory.
Runtime credentials are synthetic. Do not copy real secrets into this harness.
A `PASS` file is written only after all assertions and cleanup succeed.

An exclusive artifact-directory lock rejects concurrent runs. Existing
containers/volumes are always collisions, even when labeled. Only the reserved
network can be adopted after both ownership labels match. Cleanup rechecks
labels and removes only resources created/adopted by this invocation. Container
launch attempts are tracked before dispatch; cleanup reconciles successful
inventory results so failed starts are removed and failures before allocation
are skipped. Unknown inventory/ownership failures remain errors. No global
prune or shared stack operations occur. A killed process or daemon outage may
leave resources and the lock behind: inspect the logs, exact names and labels
before manual recovery; never bypass a collision by deleting a foreign object.

These are finite local correctness checks under concurrent build load. They do
not certify remote B2/Hetzner credential policy, network, object lock, retention,
production RPO/RTO, systemd/provisioning, signed envelope validity, or application
readiness/auth/sync/export behavior. Those remain separate operational gates.

Focused orchestration regressions use the real local daemon and only the reserved
network/tools container; the pinned harness image must already exist:

```sh
python3 -B tests/release-readiness/restore/test_controls.py
```

These check fixed endpoint selection with conflicting nonexistent local socket/
connection settings, unsupported backend rejection, actual failed container
start cleanup, failure before allocation, and refusal on ownership mismatch.
The tests use the same exclusive lock and preserve logs under `restore/controls-*`.
Independent fixture cleanup rechecks labels even when the regression is RED.
They do not rerun PostgreSQL/blob restore or contact remote services.
