# Kamori hosted infrastructure

This Pulumi Go project declares the public-beta Hetzner topology and its
S3-compatible storage integration. It does not deploy from a developer machine
by default. GitHub Actions is the deployment entrypoint.

The stack creates two spread app nodes, PostgreSQL primary/standby nodes with
protected volumes, an ops/witness node, a private network, firewall, HTTPS load
balancer, Porkbun public DNS records, a Hetzner-managed TLS certificate, and an
independent Hetzner Object Storage DR bucket. The existing private Backblaze B2
primary, PostgreSQL-backup, and Pulumi-state buckets are external prerequisites;
Pulumi does not hold an account-wide B2 administration key. Kubernetes is
intentionally not used.

The production defaults use the x86 `CX23` type for app/ops nodes and `CX33`
for PostgreSQL nodes. These replace the earlier ARM `CAX11`/`CAX21` defaults,
which Hetzner rejected for every selected production location. The optional
`appServerType`, `opsServerType`, and `dbServerType` stack settings remain
available for a reviewed resize; any override must be checked against Hetzner's
current location availability before it is applied.

## Bootstrap

Follow the complete [production secret procedure](../SECRETS.md). It creates a
passphrase-encrypted `production` stack, generates the persistent OPAQUE setup,
separates infrastructure and runtime object-store credentials, configures the
GitHub `production` Environment, previews the plan, and validates a two-node
deployment. Never put a secret directly on a command line.

Pulumi state is stored in the private
`kamori-production-pulumi-state` B2 bucket through the backend URL committed in
`Pulumi.yaml`. Local operators and GitHub Actions use the same backend. Its
dedicated bucket-scoped Application Key is a bootstrap credential supplied as
AWS-compatible environment variables; it is not stored inside Pulumi config.
AWS SDK v2 request and response checksum behavior is fixed to `when_required`
for compatibility with the B2 S3 endpoint; local operators and CI must retain
those environment settings. Pulumi CLI is pinned to `3.258.0`, whose AWS
transfer manager `v0.3.5` respects them. Do not use `3.257.0`, which bundles
the affected `v0.2.4` implementation.

Create a dedicated Porkbun API credential restricted to `kamori.app` and store
both halves as Pulumi secrets. Pulumi keeps Porkbun authoritative, creates the
apex/app/API/admin A and AAAA records, delegates only their four ACME challenge
names to a protected Hetzner DNS zone, and attaches the resulting managed
certificate to the load balancer. No certificate ID or renewal job is
configured manually.
Bucket application keys must be limited to the named bucket before they are
stored as Pulumi secrets.

The DR location (`fsn1`), endpoint, and bucket name are versioned infrastructure
decisions, not stack inputs. Pulumi derives them and creates the private bucket
through Hetzner's S3 API. Only the project-bound S3 credential pair must be
generated once in Hetzner Console because credential creation is not available
through that API.

Before the first update, inspect Porkbun for existing A, AAAA, CNAME, ALIAS, or
NS records at the managed names, including `_acme-challenge`,
`_acme-challenge.app`, `_acme-challenge.api`, and `_acme-challenge.admin`.
Remove obsolete records or import records that must be preserved; duplicate
address records can split traffic between the old and new targets. Pulumi
protects the managed DNS, ACME zone, and certificate, so intentional removal
requires an explicit reviewed unprotect operation.

The dependency graph guarantees that Porkbun accepts the delegation records
before Pulumi requests the certificate, but it cannot erase DNS caches on the
internet. If first issuance reports an ACME delegation error, wait at least the
600-second DNS TTL and rerun the same protected `up`; do not upload a temporary
certificate or add a scheduled renewal workflow.

Create separate bucket-scoped Backblaze keys for the API, PostgreSQL backup,
and replication jobs. Only the API key is a Pulumi input; the other two stay on
their dedicated hosts. SSH listens on TCP port `2022` on every node. Only the
ops/bastion node exposes that port publicly, protected by key-only
authentication and Fail2ban; app and database nodes accept it only from the ops
node over the private network. No administrator source-IP configuration is
required.
Application and provider secrets stay in encrypted Pulumi config. GitHub stores
the passphrase that unlocks that config and the dedicated B2 credential needed
to reach the state before it can be unlocked. Kamori does not use Pulumi Cloud
or a `PULUMI_ACCESS_TOKEN`.

VM creation itself uses the Hetzner API, not SSH. Hetzner injects the configured
public keys, then local cloud-init validates the hardened OpenSSH configuration
and moves the daemon from the image default to `2022`. The firewall exposes
public `2022` only on the ops/bastion node and includes a private-source rule
for connections from ops to the other nodes. Do not temporarily expose `22`;
use the Hetzner Console for failed first-boot recovery.

## Application rollout

Base nodes are hardened by cloud-init and expose node-exporter only on the
private network. The manually approved `Deploy cloud server` workflow builds a
GHCR image, addresses it by digest, and rolls the two app nodes through the
self-hosted ops runner over private SSH on port `2022`. Host scripts and the
environment template live in
[`deploy/cloud-server`](../deploy/cloud-server); the Prometheus, Alertmanager,
Grafana, and ephemeral Valkey stack lives in [`deploy/ops`](../deploy/ops).
Database bootstrap/PITR assets are in [`deploy/postgres`](../deploy/postgres),
and cross-provider ciphertext replication is in
[`deploy/backup`](../deploy/backup).

The end-to-end bootstrap, secret boundaries, and release gates are in the
[`hosted-beta` runbook](../docs/runbooks/hosted-beta.md). Pulumi provisioning
alone is not evidence that replication, restore, alert delivery, or deployment
rollback works.

## Parameterized provider maintenance

Porkbun has no native Pulumi package. The native Pulumi MinIO provider also
lags the S3-compatibility behavior required by Hetzner Object Storage.
`Pulumi.yaml` therefore pins `marcfrederick/porkbun` and `aminueza/minio`; the
generated typed Go SDKs are committed under `sdks/porkbun` and `sdks/minio`,
following Pulumi's
[Any Terraform Provider](https://www.pulumi.com/docs/iac/concepts/providers/any-terraform-provider/)
workflow. CI and deployment must use the committed SDKs; never add ad-hoc HTTP
DNS or S3 mutations to the Pulumi program. Hetzner must keep `s3CompatMode`
enabled because it is S3-compatible storage, not a MinIO server.

To review and upgrade the provider, choose an explicit upstream release and
regenerate from `infra`:

```bash
pulumi package add terraform-provider marcfrederick/porkbun VERSION
pulumi package add terraform-provider aminueza/minio VERSION
go mod tidy
gofmt -w .
go test ./...
go vet ./...
```

Commit `Pulumi.yaml`, `go.mod`, and the relevant regenerated SDK directory
together. Review each upstream changelog and generated resource schema before
running a production preview; do not float provider versions in CD.

## Guardrails

`budget.go` is the versioned infrastructure budget contract. The app and alert
rules consume the same exported values. Nonessential blob delivery stops at
16 TB per month, leaving 4 TB reserved for authentication, encrypted ops,
snapshots, recovery, and controlled export. The 19 TB breaker does not shut
down the core service.

Pulumi `protect` and Hetzner deletion/rebuild protection are enabled for
PostgreSQL volumes and durable infrastructure. A deliberate two-step change is
required before destruction.
