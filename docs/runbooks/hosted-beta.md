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
   confirm that only host/volume protections change, then run `up`. This phase
   deliberately ignores changed immutable `userData`. It may create the
   dedicated configuration identity in Pulumi state, but skips host delivery:
   existing machines cannot trust that identity until the `replace` phase
   installs its public half.
2. Set `hostProvisioningPhase=replace`, preview the expected replacement of
   four VMs, two load-balancer targets, and the PostgreSQL volume attachment.
   The PostgreSQL data volume and stable ops public IP must remain unchanged.
   Then run `up`.
3. Set `hostProvisioningPhase=protect`, preview protection-only changes and run
   `up` again. Routine protected updates continue to ignore bootstrap
   `userData`; mutable files are applied through the restricted configuration
   channel. Only a future explicit `replace` phase may adopt changed bootstrap
   input by replacing a VM.

The `replace` update creates the immutable machine baseline with cloud-init,
then the same infrastructure job applies encrypted role configuration through
a separate forced-command identity. Pulumi generates PostgreSQL PKI, complete
matching SSH host keypairs and finite-lived certificates, the configuration
and deploy identities, jobs/backup/Grafana secrets, a
stable ops public IPv4, role-specific firewalls, and the private-network egress
route. App and database nodes receive no public IP addresses. `ops` provides
NAT and is the only SSH bastion. The private-host egress service configures the
default route and Hetzner's two recursive DNS resolvers before any package
installation, because the private-only interface receives no resolver through
DHCP. The resolver pair is also stored in `systemd-resolved`; every approved
host-configuration update restarts the private egress service and reconstructs
the ops NAT rules, so later Docker, network, or package restarts cannot leave a
one-shot unit marked active with stale kernel state. Release registry login and
immutable image pulls use bounded retries, but still fail closed after five
attempts.

The SSH bootstrap creates the ephemeral `/run/sshd` runtime directory before
running `sshd -t`. Ubuntu normally creates that directory through the
`RuntimeDirectory=sshd` service setting, but the explicit validation runs
before the socket-activated service has started for the first time. Raw host
key material is staged outside `/etc/ssh` and installed by `runcmd`, after
cloud-init's standard `cc_ssh` module, so image key regeneration cannot replace
the Pulumi-managed private key. The finite-lived certificate is added later
through the trusted configuration channel. The bootstrap
then stops any image-started SSH daemon before restarting the configured socket,
which prevents a process from retaining the replaced key in memory.

No operator copies files, edits `/etc/kamori`, runs bootstrap scripts, installs
a self-hosted runner, learns host keys from the network, or supplies a local PKI
path. If first boot fails, use the authenticated Hetzner Console and inspect
`/var/log/cloud-init-output.log`; do not expose port `22` or patch the machine.
Correct cloud-init and replace the empty node instead.

The first configuration connection pins the raw host keys exported by Pulumi
and explicitly requests `ssh-ed25519`; it never trusts a key learned from the
network. The role archive is read from encrypted Pulumi state, bounded to 8 MiB,
checked for path traversal and special files, and accepted only when its role
matches `/etc/kamori/node-role`. Subsequent release deployments trust the SSH
host CA. Configuration transfer is provisioning, not an availability probe.

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

The `Deploy cloud server` workflow is intentionally three separate, manually
approved actions. First run `publish` for the selected Git ref. Then run
`deploy-app-1` against that same ref, enabling `run_migrations` only for
backward-compatible expand migrations. The dedicated external monitoring
service and an operator evaluate the canary. Only after that evidence is green
run `deploy-app-2` against the same ref with migrations disabled. Each deploy
resolves the already-published tag back to immutable GHCR digests; it never
silently rebuilds a promoted release. The deployment identity cannot open an
ops shell and app sudo is limited to fixed root-owned entrypoints installed by
the infrastructure configuration phase.

The workflow rejects `run_migrations` for `deploy-app-2`. To roll back one
application node, dispatch the workflow from the older Git commit whose four
images were already published, select only that node, and leave migrations
disabled. Database migrations are expand-only and are not reversed by an
application rollback; a release is promotable only while both the previous and
new application versions can use the expanded schema.

Infrastructure and release deployment workflows perform no application
endpoint, readiness, or uptime probe. CI acceptance tests wait only for their
own disposable local stack; they never inspect the hosted service. Container
health checks coordinate runtime dependencies, Hetzner load-balancer checks own
backend routing, and the dedicated monitoring service owns public availability
and alerting.

Grafana binds only to ops localhost. An operator may view it through a local SSH
tunnel using the break-glass operator key; this is observation, not
provisioning. Valkey remains deliberately ephemeral and single-node.

## 4. Create the first operator

Run `cloud-server admin-bootstrap <username>` only in a trusted terminal on an
app host. Immediately add the printed TOTP seed to the operator authenticator,
enroll a passkey at `admin.kamori.app`, and clear the terminal. The browser
presents the available providers and the operator chooses a password manager,
platform passkey, physical security key, or another WebAuthn authenticator.
Kamori does not force an authenticator attachment or request vendor attestation;
the registration ceremony still requires WebAuthn user verification.

Sign in, add a second passkey from an independent authenticator or provider,
and verify both before opening registration. Two credentials stored only in the
same synchronized vault are not operationally independent. The control plane
rejects the transition to open registration while the acting operator has fewer
than two credentials. Operator sessions live only in browser memory for 15
minutes; every mutation requires a fresh passkey assertion, TOTP, reason, and
exact confirmation.

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
- Two operator passkeys are enrolled and tested through independent
  authenticators or providers.

Only after every gate is evidenced may an operator set the audited runtime
`registration_enabled` override to `true`. Deployment defaults remain closed.
