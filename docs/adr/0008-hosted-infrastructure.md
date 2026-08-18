# ADR 0008: Hosted infrastructure

Status: accepted

## Decision

Pulumi Go provisions Hetzner EU load balancing, two stateless app nodes,
PostgreSQL primary/standby, and an ops/witness node. Backblaze B2 EU stores live
encrypted blobs; Hetzner Object Storage holds an independent encrypted DR copy.
GitHub Actions builds and deploys immutable artifacts.

CI/CD does not perform scheduled, continuous, or post-deployment probes of
public Kamori endpoints. External TLS, HTTP availability, and uptime checks
belong to a dedicated monitoring service with independent alert delivery.
Deployment keeps only node-local readiness checks used to accept or roll back
the release. Hetzner load-balancer health checks remain part of runtime traffic
routing; neither mechanism is an external uptime monitor.

## Consequences

PostgreSQL operation, PITR, patching, and controlled failover are self-managed.
Valkey is optional and never authoritative. Cost and traffic breakers preserve
core sync/recovery while limiting nonessential blob egress.
