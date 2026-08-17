# ADR 0008: Hosted infrastructure

Status: accepted

## Decision

Pulumi Go provisions Hetzner EU load balancing, two stateless app nodes,
PostgreSQL primary/standby, and an ops/witness node. Backblaze B2 EU stores live
encrypted blobs; Hetzner Object Storage holds an independent encrypted DR copy.
GitHub Actions builds and deploys immutable artifacts.

## Consequences

PostgreSQL operation, PITR, patching, and controlled failover are self-managed.
Valkey is optional and never authoritative. Cost and traffic breakers preserve
core sync/recovery while limiting nonessential blob egress.
