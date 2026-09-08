# Backup and restore

Kamori has two independent durable classes: PostgreSQL metadata/oplog and
encrypted blob objects. Both are required for a complete restore. Content
remains ciphertext, but backups still contain personal data and receive the
same access controls as production.

## Required policy

- Archive PostgreSQL WAL continuously to a private, versioned repository.
- Take a daily base backup; retain enough WAL for at least 30 days during beta.
- Copy B2 objects to the private Hetzner DR bucket with object key, byte length,
  and SHA-256 inventory verification.
- Backup credentials are write-only where the provider supports it; restore
  credentials are separate and normally disabled.
- Never expose the Hetzner DR bucket as a client download origin.

## Installed jobs

The primary database runs `kamori-pgbackrest-backup.timer`. It performs a full
backup on Sunday, differential backups on other days, continuously archives
WAL, encrypts the repository with its own secret, and runs `pgbackrest check`.
The protected B2 repository retains five full and thirty differential backup
sets. Credentials are scoped to that bucket.

The ops node runs `kamori-blob-replication.timer`. It uses a read-only B2
credential and a write-only Hetzner credential, copies immutable ciphertext
without propagating deletions, compares complete path/size inventories, and
downloads a bounded random sample from both providers for SHA-256 comparison.
`BLOB_VERIFY_SAMPLE_COUNT` is deliberately configurable to keep verification
egress within budget.

Both jobs update `operator_job_heartbeats` through narrowly scoped identities.
A green heartbeat means the last check completed; it does not replace the
quarterly restore exercise.

PostgreSQL forces an archive switch after five idle minutes. This keeps the
PITR window moving during low traffic while limiting forced WAL growth during
an archive outage. The local volume is only a bounded buffer; stale archive
heartbeats and capacity alerts require human delivery.

## WAL archive outage or full database volume

Treat a growing `pg_wal/archive_status/*.ready` queue as an archive-path
failure, not disposable data. Never delete or move WAL files, run
`pg_resetwal`, or start PostgreSQL on a completely full filesystem.

1. Inspect the real cluster unit (`postgresql@16-main`), the mounted
   `/srv/kamori-postgres` filesystem, its `pg_wal` queue, and pgBackRest logs.
   The umbrella `postgresql.service` can remain active when the cluster failed.
2. If the private default route or DNS was lost, dispatch `Hosted
   infrastructure` with `repair-egress`. It only restores network
   configuration and performs no service or availability probe.
3. If free space is insufficient to start PostgreSQL safely, increase the
   committed `kamori:databaseVolumeSizeGB`. Run `Hosted infrastructure /
   preview` and proceed only when it shows an in-place volume grow with no
   replacement or deletion.
4. Run `Hosted infrastructure / up`. Pulumi grows the block device, and the
   delivered database configuration expands ext4 before PostgreSQL starts.
   With B2 reachable, pgBackRest archives the existing `.ready` backlog and
   PostgreSQL recycles WAL normally.
5. Inspect systemd state, filesystem usage, pgBackRest logs, and the `.ready`
   count until the queue is draining. Public endpoint checks belong to the
   dedicated monitoring service, not CI/CD.
6. Confirm that both the stale-backup and disk-capacity alerts deliver to a
   human. A configured Prometheus rule without a real Alertmanager receiver is
   not an operational alert.

## Quarterly restore exercise

1. Create isolated database and worker nodes with no public application route.
2. Select a timestamp before a recorded synthetic operation.
3. Restore the latest preceding base backup and replay WAL to that timestamp.
4. Compare database counts, constraints, migration version, and operation
   cursors with the signed exercise manifest.
5. Restore a sampled set of blob objects from Hetzner, verify stored size and
   SHA-256 against `space_blobs`, and confirm the primary B2 bucket was not read.
6. Start one app node against the isolated restore with registration disabled;
   run readiness, authentication, sync-down, and encrypted export checks.
7. Destroy the isolated stack through reviewed Pulumi changes and retain the
   redacted exercise report, timings, and failures.

Do not declare a backup successful because upload jobs ran. The release gate is
a completed restore with measured RPO/RTO and integrity verification.

Never run a destructive restore against the primary paths. The restore host
must use a fresh protected volume and a separate database endpoint. Keep
`registration_enabled=false` throughout the exercise.
