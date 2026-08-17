# Kamori hosted infrastructure

This Pulumi Go project declares the public-beta Hetzner topology and the two
private S3-compatible ciphertext buckets. It does not deploy from a developer
machine by default. GitHub Actions is the deployment entrypoint.

The stack creates two spread app nodes, PostgreSQL primary/standby nodes with
protected volumes, an ops/witness node, a private network, firewall, HTTPS load
balancer, Backblaze B2 primary bucket, and independent Hetzner Object Storage
DR bucket. Kubernetes is intentionally not used.

## Bootstrap

Follow the complete [production secret procedure](../SECRETS.md). It creates a
passphrase-encrypted `production` stack, generates the persistent OPAQUE setup,
separates infrastructure and runtime object-store credentials, configures the
GitHub `production` Environment, previews the plan, and validates a two-node
deployment. Never put a secret directly on a command line.

Create or import a Hetzner managed certificate after `api.kamori.app` points to
the load balancer, then configure its numeric ID. Bucket application keys must
be limited to the named bucket before they are stored as Pulumi secrets.

The certificate IDs must cover the apex, app, API, and admin hostnames. DNS is
kept outside this stack until its authoritative provider is chosen; the
workflow exports the load-balancer IPv4/IPv6 targets and never mutates DNS
implicitly.

The B2 Pulumi credential needs bucket-management rights and is used only by the
protected infrastructure workflow. Create separate bucket-scoped runtime keys
for the API, PostgreSQL backup, and replication jobs; never put the Pulumi key
on a host. The example IP is documentation-only. Restrict `adminCidrs` to current operator
addresses; never use `0.0.0.0/0` for SSH. Application and provider secrets stay
in encrypted Pulumi config. GitHub stores only the credential that unlocks that
config and the Pulumi backend token.

## Application rollout

Base nodes are hardened by cloud-init and expose node-exporter only on the
private network. The manually approved `Deploy cloud server` workflow builds a
GHCR image, addresses it by digest, and rolls the two app nodes through the
self-hosted ops runner. Host scripts and the environment template live in
[`deploy/cloud-server`](../deploy/cloud-server); the Prometheus, Alertmanager,
Grafana, and ephemeral Valkey stack lives in [`deploy/ops`](../deploy/ops).
Database bootstrap/PITR assets are in [`deploy/postgres`](../deploy/postgres),
and cross-provider ciphertext replication is in
[`deploy/backup`](../deploy/backup).

The end-to-end bootstrap, secret boundaries, and release gates are in the
[`hosted-beta` runbook](../docs/runbooks/hosted-beta.md). Pulumi provisioning
alone is not evidence that replication, restore, alert delivery, or deployment
rollback works.

## Guardrails

`budget.go` is the versioned infrastructure budget contract. The app and alert
rules consume the same exported values. Nonessential blob delivery stops at
16 TB per month, leaving 4 TB reserved for authentication, encrypted ops,
snapshots, recovery, and controlled export. The 19 TB breaker does not shut
down the core service.

Pulumi `protect` and Hetzner deletion/rebuild protection are enabled for
PostgreSQL volumes and durable infrastructure. A deliberate two-step change is
required before destruction.
