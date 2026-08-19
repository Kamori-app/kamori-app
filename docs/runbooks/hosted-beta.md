# Hosted beta bootstrap and deployment

This runbook keeps `KAMORI_REGISTRATION_ENABLED=false` until infrastructure,
recovery, alerting, and application checks have all passed. It assumes the
Pulumi `production` stack from [`infra`](../../infra/README.md).

## 1. Provision the base stack

Before provisioning, verify that the existing
`kamori-production-primary` and `kamori-production-postgres` Backblaze buckets
are private. The Pulumi stack treats their endpoint, region, and names as
versioned external prerequisites and does not receive an account-wide B2 key.
The API credential must be restricted to the primary bucket; PostgreSQL backup
and replication credentials are separate host-only keys documented in
`SECRETS.md`.

Run the manually approved `Hosted infrastructure` GitHub Actions workflow with
`preview`, review replacements and protected resources, then run `up`. Record
the load-balancer and private node outputs. Pulumi creates the Porkbun A/AAAA
records for all public hostnames, delegates the four corresponding ACME
challenge names to the protected Hetzner DNS zone, and requests the managed
certificate. There is no manual certificate ID or renewal workflow.

Before the first `up`, inspect the current Porkbun zone and resolve conflicts at
the apex, `app`, `api`, `admin`, `_acme-challenge`, `_acme-challenge.app`,
`_acme-challenge.api`, and `_acme-challenge.admin`. Do not leave old A/AAAA,
CNAME, ALIAS, or NS records alongside the Pulumi-managed records. After the
update, compare each challenge name's three delegated NS values and the four
public hostnames with the `publicDNSNameservers`, `loadBalancerIPv4`, and
`loadBalancerIPv6` outputs.

If the initial managed-certificate request sees cached pre-delegation DNS,
leave the declared records unchanged, wait at least the 600-second TTL, and
rerun `up`. Do not bypass the model with an uploaded certificate.

Hetzner initially boots the stock image with SSH on port `22`, but no Pulumi
step connects over SSH while a VM is being created. Local `cloud-init` writes
the hardened configuration, validates it with `sshd -t`, and reloads SSH on
port `2022` before the first operator or deployment connection. The Hetzner
firewall exposes public `2022` only on the ops/bastion node. App and database
nodes accept `2022` only from ops over the private network. No administrator
source-IP allowlist is required. If cloud-init fails, recover through the
authenticated Hetzner Console rather than temporarily exposing port `22`.

Wait for the ops node to finish cloud-init, then verify it using the public
address recorded by Pulumi:

```bash
ssh -p 2022 root@<ops-public-ip> \
  'cloud-init status --wait && sshd -T | grep -Fx "port 2022"'
```

Verify each private node through the ops bastion without copying the operator's
private key onto any server:

```bash
ssh -J root@<ops-public-ip>:2022 -p 2022 root@10.42.0.11 \
  'cloud-init status --wait && sshd -T | grep -Fx "port 2022"'
```

Repeat for `10.42.0.12`, `10.42.0.21`, and `10.42.0.22`. A timeout on `2022`
is a failed bootstrap, not a reason to enable `22`; inspect the affected node's
serial console and `/var/log/cloud-init-output.log` instead.

The managed certificate must cover `kamori.app`, `app.kamori.app`,
`api.kamori.app`, and `admin.kamori.app`. All four names resolve to the same
load balancer; the immutable edge image routes strictly by `Host`. Hetzner
renews the certificate automatically as long as all four ACME NS delegations
remain intact. Treat removal of any delegation as a production TLS outage risk.

Do not put secrets in Git, shell history, Pulumi plaintext config, or cloud-init
logs. Store Pulumi inputs with `pulumi config set --secret`; provision host
runtime secrets through an operator-owned secret channel.

## 2. Prepare the ops node

Connect as the initial operator with `ssh -p 2022 root@<ops-public-ip>`. Then:

1. Copy [`deploy/ops`](../../deploy/ops) to the ops node over port `2022`.
2. Create `/etc/kamori/ops.env` from `ops.env.example`, owned by root with mode
   `0600`.
3. Put the same random metrics token used by both app nodes in
   `/etc/kamori/secrets/metrics_token`, root-owned mode `0400`.
4. Run `sudo ./bootstrap-ops .`.
5. Reach Grafana only through an SSH tunnel:

   ```bash
   ssh -p 2022 -L 3000:127.0.0.1:3000 root@<ops-public-ip>
   ```

   Keep the session open and browse to `http://127.0.0.1:3000`.
6. Replace the placeholder Alertmanager receiver with an operator-owned
   destination and fire a synthetic alert. A silent receiver blocks release.
7. Create `/etc/kamori/backup.env` from
   `deploy/backup/backup.env.example`, provision the restricted TLS identity,
   and run `sudo deploy/backup/install-backup-worker deploy/backup`.

Valkey is deliberately ephemeral and single-node. Losing it may abort active
login handshakes and rate-limit windows, but PostgreSQL remains authoritative.

## 3. Prepare app nodes

On each app node, create `/etc/kamori/cloud.env` from
`deploy/cloud-server/cloud.env.example`, root-owned mode `0600`. Use a
bucket-scoped B2 key and keep registration disabled. Pulumi derives the
database URL and safely encodes `databasePassword`; operators must not assemble
that URL manually. It does the same for `valkeyPassword`; use its raw value as
`VALKEY_PASSWORD` in `/etc/kamori/ops.env` and do not construct the Valkey URL
yourself. The metrics bearer token must match the ops secret.

Generate `KAMORI_ADMIN_TOTP_KEK` once as 32 random bytes encoded with standard
base64. Store it in the secret system and on both app nodes; losing or changing
it makes operator TOTP seeds unrecoverable. It is separate from JWT and user
content keys. Generate an independent `KAMORI_AUTH_TOTP_KEK` for consumer TOTP
seeds; never reuse either value as the other, a JWT secret, or backup password.

Generate the private PostgreSQL PKI with `deploy/postgres/generate-pki` as
documented in `SECRETS.md`. Pulumi installs the CA plus the dedicated app-client
certificate/key at `/etc/kamori/postgres-{ca,client}.{crt,key}`. The private key
must be readable only by root and numeric container uid `10001`; never reuse the
replication or backup identities. The database URL uses `sslmode=verify-full`,
and the generated primary server certificate contains `10.42.0.21` in its SAN.

The `Deploy cloud server` workflow runs on a self-hosted runner carrying the
`kamori-beta` label on the ops node. Give that runner a deploy-only SSH key for
the two private app IPs on TCP port `2022` and pinned nonstandard-port host keys
through the protected `production` GitHub environment. It must not hold
database, JWT, B2, or Pulumi secrets.

The workflow builds and pushes immutable API, web, operator-console, and edge
digests, applies migrations once, then rolls app node 1 and app node 2 as one
release. Each node checks `/health/ready`; a failed check restores the complete
previous release automatically. CI/CD does not probe public endpoints after
deployment; the dedicated external monitoring service owns that responsibility.

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

- PostgreSQL primary/standby replication and fencing are configured and the
  failover runbook has passed on disposable data.
- A PITR restore and a B2-to-Hetzner blob restore have been timed and verified.
- The `postgres_backup`, `blob_replication`, and `object_cleanup` heartbeats are
  current and green in the operator console.
- Both app targets independently pass readiness through the load balancer.
- Prometheus sees both app nodes and all five node exporters.
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
