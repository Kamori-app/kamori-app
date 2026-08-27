# Kamori hosted infrastructure

This Pulumi Go project declares the public-beta Hetzner topology and its
S3-compatible storage integration. It does not deploy from a developer machine
by default. GitHub Actions is the deployment entrypoint.

The stack creates two spread app nodes, one PostgreSQL primary with a protected
volume and continuous PITR archives, an ops node, a private network, firewall,
HTTPS load balancer, Porkbun public DNS records, a Hetzner-managed TLS
certificate, and an independent Hetzner Object Storage DR bucket. The existing
private Backblaze B2
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
Pulumi CLI and the Go SDK are pinned to `3.259.0`. The backend deliberately uses
AWS SDK v2 with optional request and response checksums limited to
`when_required`; local operators and CI must keep this complete contract
aligned. It has completed production previews and updates against the B2
bucket. Upgrade only after a newer release succeeds at both `preview` and a
no-op `up`; changing the pin does not migrate state.

A preview exercises state reads but not the update-lock write. If B2 rejects a
lock `PutObject` during a provider incident, Pulumi stops before changing any
infrastructure resource. Preserve the run and request ID for provider support
and rerun the reviewed operation only after B2 recovers; do not paper over the
failure with automatic retries or unreviewed backend changes.

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
and blob-replication jobs. All three are encrypted Pulumi inputs; the protected
host-configuration channel delivers each credential only to the role that
needs it. SSH listens on TCP
port `2022` on every node. Only the ops/bastion node has a public address and
exposes that port, protected by key-only authentication and Fail2ban. App and
database nodes have private addresses only and route required outbound traffic
through the ops NAT gateway. Their boot-time egress service also assigns
Hetzner's two recursive DNS resolvers to the private interface before package
installation. The same resolver pair is persisted in `systemd-resolved`, and
the reviewed host-configuration channel reapplies both private routes and NAT
rules whenever the desired role configuration changes. An unchanged role
archive is identified by its last successfully applied SHA-256 fingerprint and
does not restart services or repeat bootstrap work. Private-network DHCP does
not provide resolver addresses. No administrator source-IP configuration is
required.
Application and provider secrets stay in encrypted Pulumi config. GitHub stores
the passphrase that unlocks that config and the dedicated B2 credential needed
to reach the state before it can be unlocked. Kamori does not use Pulumi Cloud
or a `PULUMI_ACCESS_TOKEN`.

VM creation itself uses the Hetzner API, not SSH. Pulumi generates a persistent
SSH host CA, a host identity per node, a dedicated configuration identity, and
a separate release-deployment identity. Cloud-init installs each generated
private key and its matching raw public key before validating the hardened
OpenSSH configuration. The files are first staged under root-only
`/var/lib/kamori/bootstrap` and installed into `/etc/ssh` from `runcmd`, after
Ubuntu's `cc_ssh` module has finished deleting and generating image host keys.
This ordering prevents cloud-init from replacing the Pulumi-managed identity.
Any image-started SSH daemon is stopped
after validation so the socket-activated process cannot retain an older key in
memory.
Before that validation, cloud-init creates `/run/sshd` with the same ownership
and permissions declared by Ubuntu's `ssh.service`; the runtime directory does
not otherwise exist before the socket-activated service first starts.
Ubuntu's native socket activation remains enabled. A systemd drop-in
clears the image's default listener and binds `ssh.socket` only to `2022`
before installing additional packages. There is no
`ssh-keyscan` trust-on-first-use step. The operator key configured in Hetzner
remains the break-glass identity. Do not temporarily expose `22`; use the
Hetzner Console for failed first-boot recovery.

Pulumi also generates the private PostgreSQL CA, server/client certificates,
finite-lived SSH host certificates, the restricted jobs-role password, the
pgBackRest repository cipher, and the Grafana administrator password. These
values are encrypted in Pulumi state. After resource creation, the same
infrastructure workflow sends host configuration through a distinct,
forced-command SSH identity and a root-owned, role-checking installer. App and
database hosts are reached only through the ops bastion. Rotating a runtime
secret or a PostgreSQL/SSH leaf certificate therefore updates hosts in place;
it does not put secrets in GitHub and does not replace a VM. No local PKI
directory or manual `/etc/kamori` edits are part of provisioning.

Each host records the SHA-256 fingerprint only after its complete role
activation succeeds. The same encrypted archive on a later `up` is therefore a
no-op on that host, while an interrupted or changed configuration is applied
again. This keeps a no-change infrastructure update from reinstalling packages,
restarting PostgreSQL, pulling ops containers, or restarting app services.
Every regular file is staged in its destination directory and atomically
renamed into place. In particular, the installer never truncates its own live
script inode while Bash is still reading it.

## Application rollout

Cloud-init creates only the immutable host baseline, raw SSH trust, and the
restricted configuration channel. The infrastructure workflow then applies
the encrypted role configuration and exposes node-exporter only on the private
network. The manually approved `Deploy cloud server` workflow publishes four
GHCR images once, then activates the exact immutable digests on one explicitly
selected app node per dispatch. Promotion is `publish`, `deploy-app-1`, external
monitoring and operator review, then `deploy-app-2`; CI/CD does not infer health
from a successful process start. The workflow logs in with its short-lived
GitHub token and may invoke only
preinstalled root-owned deployment entrypoints through the restricted `deploy`
account. The bastion key permits only forwarding to the two app SSH endpoints.
The workflow never copies runner-controlled executable code into a privileged
path and performs no deployment or uptime probes. Host scripts and the
environment template live in
[`deploy/cloud-server`](../deploy/cloud-server); the Prometheus, Alertmanager,
Grafana, and ephemeral Valkey stack lives in [`deploy/ops`](../deploy/ops).
Valkey runs directly as the pinned image's non-root UID/GID; its root filesystem
stays read-only and only its non-persistent `/data` tmpfs is writable.
Database bootstrap/PITR assets are in [`deploy/postgres`](../deploy/postgres),
and cross-provider ciphertext replication is in
[`deploy/backup`](../deploy/backup).

The same workflow exposes a separate `repair-egress` recovery action for a
host whose route, resolver, or NAT rules have become stale. It performs no
Pulumi update and uses the existing forced-command configuration identity to
restart only the root-owned egress units: ops first, followed by the database
and both app nodes. It cannot run a shell, pull or restart application
containers, bootstrap PostgreSQL, access object-storage configuration, or
perform an availability probe. SSH transport failures are retried while a host
is temporarily unreachable. Authentication and pinned-host-key failures stop
immediately because retries cannot repair invalid trust material; a command
that reached the host and failed likewise exits immediately instead of
repeating a deterministic server-side error. Every successful configuration
install also restores the exact owner and modes of the `deploy` home, SSH
directory, and authorized-key file before validating and restarting SSH.

PostgreSQL bootstrap fingerprints the rendered pgBackRest repository
configuration. A new host, changed repository credential, bucket, endpoint, or
cipher must pass `stanza-create` and `pgbackrest check` before that fingerprint
is accepted. Routine host updates with the same fingerprint do not block on a
second repository check; the scheduled backup job continues to check the
archive and report its heartbeat.

The end-to-end bootstrap, secret boundaries, and release gates are in the
[`hosted-beta` runbook](../docs/runbooks/hosted-beta.md). Pulumi provisioning
alone is not evidence that PITR restore, blob replication, or alert delivery
works.

## Controlled first replacement

`hostProvisioningPhase` makes the initial replacement of the empty beta hosts
explicit and reviewable:

1. `retire` disables Pulumi and Hetzner delete/rebuild protection without
   changing host configuration or adopting changed immutable `userData`. The
   workflow also skips the post-update host delivery because the dedicated
   configuration identity is not trusted until replacement.
2. `replace` installs the generated raw identities and restricted configuration
   channel, recreates the four empty servers, preserves and reattaches the
   PostgreSQL volume, removes public networking from app/DB, and then applies
   each role configuration in place.
3. `protect` reenables Pulumi and Hetzner protection after the replacement is
   complete. Routine protected updates ignore later bootstrap `userData`
   changes and deliver mutable assets only through the host-configuration
   channel.

Each transition requires a separate protected Pulumi preview and update. The
application deployment remains a separate manually approved workflow. CI/CD
does not perform service probes.

After the initial `replace` update has installed the restricted host assets,
the `repair-egress` workflow action is safe to repeat and does not depend on a
new preview. It is rejected during `retire`, when the dedicated configuration
identity is not yet trusted by the replacement hosts.

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

The beta intentionally runs one PostgreSQL primary to control cost. This trades
automatic database failover for a tested PITR recovery path.
