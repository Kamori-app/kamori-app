# Kamori hosted infrastructure

This Pulumi Go project declares the public-beta Hetzner topology and the two
private S3-compatible ciphertext buckets. It does not deploy from a developer
machine by default. GitHub Actions is the deployment entrypoint.

The stack creates two spread app nodes, PostgreSQL primary/standby nodes with
protected volumes, an ops/witness node, a private network, firewall, HTTPS load
balancer, Backblaze B2 primary bucket, and independent Hetzner Object Storage
DR bucket. Kubernetes is intentionally not used.

## Bootstrap

Create or import a Hetzner managed certificate after `api.kamori.app` points to
the load balancer, then configure its numeric ID. Bucket application keys must
be limited to the named bucket before they are stored as Pulumi secrets.

```bash
cd infra
pulumi stack init beta
pulumi config set kamori:sshKeys key-name-1,key-name-2
pulumi config set kamori:adminCidrs 203.0.113.10/32
pulumi config set kamori:tlsCertificateIds 123456
pulumi config set kamori:b2Endpoint s3.eu-central-003.backblazeb2.com
pulumi config set kamori:b2Region eu-central-003
pulumi config set kamori:b2Bucket kamori-beta-primary
pulumi config set kamori:b2PostgresBackupBucket kamori-beta-postgres
pulumi config set kamori:hetznerObjectEndpoint fsn1.your-objectstorage.com
pulumi config set kamori:hetznerObjectRegion fsn1
pulumi config set kamori:hetznerObjectBucket kamori-beta-dr
pulumi config set --secret kamori:hcloudToken '...'
pulumi config set --secret kamori:b2InfraKeyId '...'
pulumi config set --secret kamori:b2InfraApplicationKey '...'
pulumi config set --secret kamori:hetznerObjectAccessKey '...'
pulumi config set --secret kamori:hetznerObjectSecretKey '...'
pulumi preview
```

The certificate IDs must cover the apex, app, API, and admin hostnames. DNS is
kept outside this stack until its authoritative provider is chosen; the
workflow exports the load-balancer IPv4/IPv6 targets and never mutates DNS
implicitly.

The B2 Pulumi credential needs bucket-management rights and is used only by the
protected infrastructure workflow. Create separate bucket-scoped runtime keys
for the API, PostgreSQL backup, and replication jobs; never put the Pulumi key
on a host. The example IP is documentation-only. Restrict `adminCidrs` to current operator
addresses; never use `0.0.0.0/0` for SSH. Production secrets belong in the
GitHub environment and Pulumi encrypted config, not committed stack files.

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
