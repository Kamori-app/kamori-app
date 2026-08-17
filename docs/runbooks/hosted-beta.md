# Hosted beta bootstrap and deployment

This runbook keeps `KAMORI_REGISTRATION_ENABLED=false` until infrastructure,
recovery, alerting, and application checks have all passed. It assumes the
Pulumi `beta` stack from [`infra`](../../infra/README.md).

## 1. Provision the base stack

Run the manually approved `Hosted infrastructure` GitHub Actions workflow with
`preview`, review replacements and protected resources, then run `up`. Record
the load-balancer and private node outputs. Point `api.kamori.app` at the load
balancer and verify the managed certificate before any application rollout.

The managed certificate must cover `kamori.app`, `app.kamori.app`,
`api.kamori.app`, and `admin.kamori.app`. All four names resolve to the same
load balancer; the immutable edge image routes strictly by `Host`.

Do not put secrets in Git, shell history, Pulumi plaintext config, or cloud-init
logs. Store Pulumi inputs with `pulumi config set --secret`; provision host
runtime secrets through an operator-owned secret channel.

## 2. Prepare the ops node

1. Copy [`deploy/ops`](../../deploy/ops) to the ops node.
2. Create `/etc/kamori/ops.env` from `ops.env.example`, owned by root with mode
   `0600`.
3. Put the same random metrics token used by both app nodes in
   `/etc/kamori/secrets/metrics_token`, root-owned mode `0400`.
4. Run `sudo ./bootstrap-ops .`.
5. Reach Grafana only through an SSH tunnel to `127.0.0.1:3000`.
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
bucket-scoped B2 key, URL-encode database and Valkey passwords, and keep
registration disabled. The metrics bearer token must match the ops secret.

Generate `KAMORI_ADMIN_TOTP_KEK` once as 32 random bytes encoded with standard
base64. Store it in the secret system and on both app nodes; losing or changing
it makes operator TOTP seeds unrecoverable. It is separate from JWT and user
content keys. Generate an independent `KAMORI_AUTH_TOTP_KEK` for consumer TOTP
seeds; never reuse either value as the other, a JWT secret, or backup password.

Install the PostgreSQL CA plus an app-client certificate/key at
`/etc/kamori/postgres-{ca,client}.{crt,key}`. The private key must be readable
only by root and numeric container uid `10001`; never reuse the replication or
backup identities. The database URL uses `sslmode=verify-full`, so the server
certificate needs the private IP in its SAN.

The `Deploy cloud server` workflow runs on a self-hosted runner carrying the
`kamori-beta` label on the ops node. Give that runner a deploy-only SSH key for
the two private app IPs and pinned host keys through the protected `beta`
GitHub environment. It must not hold database, JWT, B2, or Pulumi secrets.

The workflow builds and pushes immutable API, web, operator-console, and edge
digests, applies migrations once, then rolls app node 1 and app node 2 as one
release. Each node checks `/health/ready`; a failed check restores the complete
previous release automatically.

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
- The public uptime monitor checks `/health/ready` from outside Hetzner.
- The GitHub-hosted `Hosted endpoint probe` succeeds and its failure
  notifications reach a human. It is the beta external probe, not an SLA-grade
  monitoring substitute; replace or supplement it before paid B2B commitments.
- Legal templates are reviewed and a real operator exists; until then public
  registration remains closed regardless of technical readiness.
- Two operator security keys are enrolled and tested from separate storage
  locations.

Only after every gate is evidenced may an operator set the audited runtime
`registration_enabled` override to `true`. Deployment defaults remain closed.
