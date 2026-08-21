# Hosted beta bootstrap and deployment

This runbook keeps `KAMORI_REGISTRATION_ENABLED=false` until infrastructure,
recovery, alerting, and application checks have all passed. It assumes the
Pulumi `production` stack from [`infra`](../../infra/README.md).

## 1. Supply external object-storage identities

Verify that `kamori-production-primary` and `kamori-production-postgres` are
private. The Pulumi stack deliberately has no account-wide Backblaze key, so an
operator creates two bucket-scoped keys once:

1. `kamori-production-postgres`: only `kamori-production-postgres`, Read and
   Write, S3 bucket listing enabled.
2. `kamori-production-replication`: only `kamori-production-primary`, Read
   Only, S3 bucket listing enabled.

Enter their `keyID` and one-time `applicationKey` values with the four hidden
`pulumi config set --secret` commands in `SECRETS.md`. Never copy these values
to a host or GitHub. Pulumi installs them only on DB and ops respectively.

## 2. Replace and protect the empty hosts

The first rollout is deliberately split into three protected infrastructure
updates:

1. With `hostProvisioningPhase=retire`, run `Hosted infrastructure / preview`,
   confirm that only host/volume protections change, then run `up`.
2. Set `hostProvisioningPhase=replace`, preview the expected replacement of
   four VMs, two load-balancer targets, and the PostgreSQL volume attachment.
   The PostgreSQL data volume and stable ops public IP must remain unchanged.
   Then run `up`.
3. Set `hostProvisioningPhase=protect`, preview protection-only changes and run
   `up` again.

The `replace` update performs the complete bootstrap through role-specific
cloud-init. Pulumi generates PostgreSQL PKI, complete matching SSH host
keypairs and certificates, the deploy identity, jobs/backup/Grafana secrets, a
stable ops public IPv4, role-specific firewalls, and the private-network egress
route. App and database nodes receive no public IP addresses. `ops` provides
NAT and is the only SSH bastion.

No operator copies files, edits `/etc/kamori`, runs bootstrap scripts, installs
a self-hosted runner, learns host keys from the network, or supplies a local PKI
path. If first boot fails, use the authenticated Hetzner Console and inspect
`/var/log/cloud-init-output.log`; do not expose port `22` or patch the machine.
Correct cloud-init and replace the empty node instead.

Public TLS remains separate: Pulumi manages Porkbun records and ACME
delegations, while Hetzner issues, attaches, and renews the load-balancer
certificate. Caddy listens only on private `:8080`. There is no certificate ID,
certificate upload, or renewal workflow.

## 3. Connect GitHub deployment and release containers

After the `replace` update, transfer only the generated deploy private key and
public host CA into the protected GitHub `production` Environment, and transfer
the stable ops IPv4 as a variable. Use the exact `pulumi stack output | gh`
commands in `SECRETS.md`. App private addresses remain `10.42.0.11` and
`10.42.0.12`.

The `Deploy cloud server` workflow uses a GitHub-hosted runner. It reaches the
app nodes through the certificate-authenticated ops bastion, uses the job-scoped
`GITHUB_TOKEN` to pull immutable GHCR digests, applies migrations once, and
rolls app node 1 followed by app node 2. The deployment identity cannot open an
ops shell and app sudo is limited to fixed root-owned entrypoints installed by
cloud-init.

CI/CD performs no endpoint, readiness, or uptime probe. Container health checks
only coordinate local runtime dependencies, Hetzner load-balancer checks own
backend routing, and the dedicated monitoring service owns public availability
and alerting.

Grafana binds only to ops localhost. An operator may view it through a local SSH
tunnel using the break-glass operator key; this is observation, not
provisioning. Valkey remains deliberately ephemeral and single-node.

## 4. Create the first operator

Run `cloud-server admin-bootstrap <username>` only in a trusted terminal on an
app host. Immediately add the printed TOTP seed to the operator authenticator,
enroll a supported roaming security key at `admin.kamori.app`, and clear the
terminal. The current strict attestation catalog accepts supported Yubico
devices; this is an explicit beta allowlist, not a claim that other keys are
insecure.

Sign in, add a second independently stored roaming key, and verify both keys
before opening registration. The control plane rejects the transition to open
registration while the acting operator has fewer than two keys. Operator
sessions live only in browser memory for 15 minutes; every mutation requires a
fresh key assertion, TOTP, reason, and exact confirmation.

## 5. Release gates

- PostgreSQL WAL archiving and scheduled encrypted backups are configured.
- A PostgreSQL PITR restore and a B2-to-Hetzner blob restore have been timed
  and verified.
- The `postgres_backup`, `blob_replication`, and `object_cleanup` heartbeats are
  current and green in the operator console.
- Both app targets independently pass readiness through the load balancer.
- Prometheus sees both app nodes and all four node exporters.
- Alertmanager delivers a synthetic critical alert to a human.
- Quota alerts at account 80/95% and egress 10/14 TB are loaded.
- The dedicated external monitoring service checks API readiness, user web,
  operator console, DNS, and TLS from outside Hetzner, and its failure
  notifications reach a human. GitHub Actions is not used for uptime checks.
- Legal templates are reviewed and a real operator exists; until then public
  registration remains closed regardless of technical readiness.
- Two operator security keys are enrolled and tested from separate storage
  locations.

Only after every gate is evidenced may an operator set the audited runtime
`registration_enabled` override to `true`. Deployment defaults remain closed.
