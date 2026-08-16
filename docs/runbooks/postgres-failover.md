# PostgreSQL controlled failover

The beta topology uses asynchronous streaming replication. A failover can lose
transactions not yet replayed on the standby; it is never an automatic
split-brain decision.

## Preconditions

- Confirm the incident, replication lag, last received/replayed LSN, and which
  node is reachable.
- Stop both app nodes or remove both load-balancer targets before promotion.
- Fence the old primary at the Hetzner control plane. Network uncertainty is
  not proof that the old primary is dead.
- Record timestamps, LSNs, operator identity, and every command in the incident
  log without copying secrets.

Initial primary/standby installation is performed from `deploy/postgres` after
root-owned environment and per-role TLS files are provisioned. Standby
initialization requires the exact `CONFIRM_EMPTY_STANDBY` phrase because it
empties only the explicitly validated protected-volume data directory. These
bootstrap scripts are not failover automation.

## Promotion sequence

1. Snapshot current infrastructure state and disable automated deployments.
2. Power off or firewall-fence the old primary and verify it cannot accept
   client or replication connections.
3. On the standby, verify recovery state and replay position, then promote with
   `pg_ctlcluster <version> main promote`.
4. Confirm the promoted node is writable and that expected committed sentinel
   records exist.
5. Change the application database endpoint through reviewed configuration;
   roll one app node and verify readiness plus signed-op append/read.
6. Roll the second app node and restore load-balancer membership.
7. Rebuild the old primary as a fresh standby from the new primary. Never start
   its old data directory as a second primary.

## Abort conditions

Stop and restore from backup if the old primary cannot be fenced, the standby
timeline is ambiguous, integrity checks fail, or required migration state is
missing. Availability does not override consistency.

The exact PostgreSQL version, TLS/replication configuration, WAL repository,
and volume mount paths must be captured in the environment-specific private
runbook after the first disposable-stack rehearsal.

After promotion, create a new physical replication slot before rebuilding the
standby, update the pgBackRest stanza ownership if the backup source changes,
and run `pgbackrest check`. A successful SQL promotion without a valid WAL
archive is not a completed recovery.
