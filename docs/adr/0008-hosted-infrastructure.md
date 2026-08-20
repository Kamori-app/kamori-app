# ADR 0008: Hosted infrastructure

Status: accepted

## Decision

Pulumi Go provisions Hetzner EU load balancing, two stateless app nodes,
one PostgreSQL primary, and an ops node. Backblaze B2 EU stores live
encrypted blobs; Hetzner Object Storage holds an independent encrypted DR copy.
GitHub Actions builds and deploys immutable artifacts.

CI/CD does not perform scheduled, continuous, or post-deployment probes of
public Kamori endpoints. External TLS, HTTP availability, and uptime checks
belong to a dedicated monitoring service with independent alert delivery.
Deployment does not perform endpoint or readiness probes. Container health
checks and Hetzner load-balancer health checks remain part of runtime traffic
routing; neither mechanism runs in CI/CD or acts as the external uptime monitor.

## Consequences

PostgreSQL operation, PITR, patching, and restore are self-managed. The beta
accepts database recovery time instead of paying for an idle replica; a tested
encrypted PITR repository is therefore a release gate. Valkey is optional and
never authoritative. Cost and traffic breakers preserve core sync/recovery
while limiting nonessential blob egress.
