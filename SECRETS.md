# Production secrets

Kamori stores deployment-owned secret values in Pulumi encrypted stack
configuration. GitHub Actions stores only the credentials needed to unlock the
Pulumi stack and reach the infrastructure providers. User keys, device keys,
refresh tokens, and security-space keys never belong in Pulumi or GitHub.

## Security boundary

- `Pulumi.production.yaml` contains ciphertext under `secure:` keys and may be
  committed.
- Pulumi state is stored in the private, manually managed
  `kamori-production-pulumi-state` B2 bucket. The bucket is a bootstrap
  prerequisite and is intentionally not managed by the stack whose state it
  contains.
- `PULUMI_CONFIG_PASSPHRASE` is stored only in the protected GitHub
  `production` Environment and in the offline recovery record.
- The raw OPAQUE server setup is stored only as a Pulumi secret and as an
  encrypted offline recovery copy. Pulumi writes it to app hosts as
  `/etc/kamori/secrets/opaque-server-setup` with mode `0400` and numeric owner
  `10001:10001`; the container mounts it read-only.
- The refresh-rotation key follows the same path and is mounted read-only as
  `/run/secrets/refresh-rotation-key`. It never appears in a container image or
  plaintext Pulumi stack file.
- Never put a raw secret in `main.go`, a workflow file, a plain Pulumi config
  value, an Actions variable, an issue, or a pull request.

## 1. Create the production stack

### `PULUMI_CONFIG_PASSPHRASE` — local stack encryption passphrase

- **Classification:** high-value operator secret.
- **Purpose:** derives the encryption key used for Pulumi configuration secrets
  and passphrase-protected values in state.
- **Dependencies:** every local preview/update, GitHub infrastructure run, and
  disaster recovery operation needs it. It does not grant provider access by
  itself; the Pulumi backend state and provider credentials are separate.
- **Value:** a new high-entropy passphrase generated for this production stack,
  not a reused account password.
- **Storage:** keep one copy in the protected GitHub `production` Environment
  and one independent encrypted offline recovery record. Do not commit it or
  put it in a shell profile.
- **Loss or rotation:** losing every copy makes encrypted stack data
  unrecoverable. Rotate only with `pulumi stack change-secrets-provider
  passphrase` and immediately update both storage locations.

The repository fixes the backend URL in `infra/Pulumi.yaml`. Both local
operators and GitHub Actions therefore use the same B2 state, locks, and
history. The backend needs a dedicated B2 Application Key restricted to the
private `kamori-production-pulumi-state` bucket. Do not reuse the API runtime
key or the PostgreSQL-backup key.

Use Pulumi CLI `3.244.0`, which is pinned in `infra/Pulumi.yaml` and the
infrastructure workflow. It is the last release before Pulumi moved this DIY
backend to the AWS SDK v2 upload path. With this B2 endpoint, CLI `3.257.0`
fails while writing the update lock because B2 rejects its checksum-algorithm
header; CLI `3.258.0` gets past that incompatibility but B2 rejects its
`STANDARD` storage-class header. Both failures happen before any provider
resource is changed. The Go SDK dependency can remain at `3.258.0`; this
compatibility pin applies only to the CLI that owns the DIY backend.

Treat the pin as temporary and explicit, not as a floating downgrade. Test a
newer CLI with both a preview and a no-op update against this bucket before
changing both pins. A preview alone is insufficient because it does not
exercise the failing update-lock write.

Install the pinned CLI with Pulumi's official installer and call that exact
binary if another package manager still exposes a newer release first in
`PATH`:

```bash
curl -fsSL https://get.pulumi.com | sh -s -- --version 3.244.0
~/.pulumi/bin/pulumi version
```

Export the backend credentials and passphrase into the current Fish session.
The silent `read` commands avoid putting either secret in shell history:

```fish
set -gx AWS_ACCESS_KEY_ID 'YOUR_PULUMI_BUCKET_KEY_ID'
read -gx -s -P 'B2 Pulumi application key: ' AWS_SECRET_ACCESS_KEY
echo
read -gx -s -P 'Pulumi config passphrase: ' PULUMI_CONFIG_PASSPHRASE
echo
set -gx AWS_REGION eu-central-003
```

`AWS_ACCESS_KEY_ID` is an identifier rather than a secret. Do not persist
`AWS_SECRET_ACCESS_KEY` or `PULUMI_CONFIG_PASSPHRASE` with Fish `set -U` or
`set -Ux`: universal variables are stored unencrypted on disk. Load them from
the password manager for each administrative session instead. Do not add
`awssdk=v2` to the backend URL or AWS SDK v2 checksum compatibility variables:
the pinned CLI deliberately uses its older AWS SDK v1 backend implementation.

Authenticate to the configured B2 backend, register the existing logical
production stack there, and verify the selected backend:

```bash
cd infra
pulumi login 's3://kamori-production-pulumi-state?endpoint=s3.eu-central-003.backblazeb2.com&region=eu-central-003&s3ForcePathStyle=true'
pulumi stack init production --secrets-provider=passphrase
pulumi stack select production
pulumi whoami --verbose
```

This creates only the state record in B2; it does not create infrastructure.
The previous local `file://~` state contains zero resources and does not need an
export/import migration. Keep it untouched until the first B2-backed preview
has succeeded.

Use the existing high-entropy production passphrase when prompted. Keep it in
the offline password-manager or recovery record. Losing both the passphrase
and the recovery copy makes the encrypted stack configuration unrecoverable.

If a B2-backed production stack already exists but uses a different secrets
provider, change only its secrets provider instead of initializing it again:

```bash
cd infra
pulumi stack select production
pulumi stack change-secrets-provider passphrase
```

## 2. Generate the OPAQUE server setup

### `kamori:opaqueServerSetup` — OPAQUE password-authentication root

- **Classification:** long-lived, high-value server cryptographic secret.
- **Purpose:** lets every API node process OPAQUE registration and login without
  receiving or storing a user's plaintext password. Existing OPAQUE password
  records are cryptographically bound to the setup version that created them.
- **Dependencies:** both app nodes and every replacement node must receive the
  identical value. The startup fingerprint guard rejects a different setup.
- **Loss:** users whose password records depend on the lost setup cannot use
  password authentication; the Pulumi copy and encrypted offline copy are both
  required recovery assets.
- **Rotation:** do not rotate routinely. Rotation requires versioned setup
  support and an authenticated password-record migration.

Build the server from the reviewed revision and generate the setup exactly
once:

```bash
cargo build --locked -p cloud-server --release
./target/release/cloud-server opaque-setup generate
```

The command prints one standard-base64 value. Capture it directly in a secure
terminal session and add it as an encrypted Pulumi value:

```bash
cd infra
pulumi config set --secret kamori:opaqueServerSetup
```

Paste the generated value at the hidden prompt. Do not pass it as a command-line
argument: command-line values may be retained in shell history or process
inspection logs.

### `kamori:refreshRotationKey` — deterministic refresh retry key

- **Classification:** 32-byte runtime HMAC key.
- **Purpose:** derives the same replacement refresh token when an identical
  rotation request is retried within the idempotency window. This prevents a
  lost response from being misclassified as token reuse while retaining reuse
  detection for genuinely replayed tokens.
- **Dependencies:** every API node must use the same key. Existing sessions and
  refresh rotation behavior depend on it.
- **Loss or rotation:** replacing it requires revoking every refresh session in
  the same maintenance window. It is unrelated to JWT signing and OPAQUE.

Create the independent refresh-rotation key and paste its standard-base64 output
at the second hidden prompt:

```bash
openssl rand -base64 32
cd infra
pulumi config set --secret kamori:refreshRotationKey
```

Do not reuse the JWT, TOTP, database, or OPAQUE secret for this purpose.

Export an encrypted stack backup after the value is stored:

```bash
pulumi stack export --show-secrets=false --file pulumi-production-backup.json
```

Store the export with the offline recovery record. The export does not replace
the OPAQUE setup recovery copy or the Pulumi passphrase.

## 3. Add provider and application secrets

Run every command in this section from `infra`. Commands without `--secret`
store public topology metadata in `Pulumi.production.yaml`. Commands with
`--secret` open a hidden value prompt and store only ciphertext. Placeholder
values below must be replaced; never paste a real secret on the command line.

The primary, PostgreSQL-backup, and Pulumi-state Backblaze buckets exist outside
this stack. Pulumi does not receive an account-wide B2 administration key and
does not create, delete, or reconfigure them. The infrastructure program
receives only the existing primary-bucket runtime key needed by the API. Access
to the state bucket is supplied separately to the Pulumi CLI before it can load
stack configuration.

### `kamori:sshKeys` — Hetzner SSH public-key references

- **Classification:** non-secret configuration.
- **Value:** a comma-separated list of SSH key names or numeric IDs already
  registered in the production Hetzner project. `operator-key` is an example,
  not a literal required name.
- **Purpose:** Hetzner injects the referenced public keys while creating each
  VM. Pulumi creates the servers through the Hetzner API and does not need an
  initial SSH connection.
- **Dependencies:** the matching private key remains only on an operator
  device. The ops node is the sole public SSH endpoint on TCP `2022`; app and
  database nodes accept `2022` only from ops at `10.42.0.31/32`.
- **Loss or rotation:** losing every matching private key leaves Hetzner Console
  as the recovery path. Add and verify a replacement key before removing the
  previous one.

```bash
pulumi config set kamori:sshKeys operator-key
```

The stock image may briefly start `sshd` on `22`, but the Hetzner firewall
never exposes that port. Local cloud-init validates the configuration with
`sshd -t` and reloads it on `2022`. If this fails, inspect
`/var/log/cloud-init-output.log` through the authenticated Hetzner Console; do
not expose `22` as a workaround.

TLS certificate IDs are not configuration. Pulumi creates a protected Hetzner
DNS zone used only for ACME validation, delegates the four challenge names for
the apex/app/API/admin certificate from Porkbun, requests a Hetzner managed
certificate, and passes the resulting provider ID directly to the load-balancer
HTTPS service. Hetzner renews the certificate while every delegation remains
valid. Do not upload a certificate or copy an ID into the stack.

### Existing Backblaze B2 storage

The reviewed storage topology is versioned in `infra/storage.go` and is not
entered into Pulumi config:

| Purpose | Bucket | Region / endpoint |
|---|---|---|
| Encrypted application blobs | `kamori-production-primary` | `eu-central-003` / `s3.eu-central-003.backblazeb2.com` |
| PostgreSQL backups and WAL | `kamori-production-postgres` | `eu-central-003` / `s3.eu-central-003.backblazeb2.com` |

In the Backblaze console, verify both buckets are **Private**. Do not enable
public file listing or public object access. A region, endpoint, or bucket-name
change is a reviewed storage migration in code, not a configuration edit.

The existing `kamori-production` application key is the API runtime identity.
Verify that **Allow access to Bucket(s)** names only
`kamori-production-primary` and **Type of Access** is **Read and Write**. Leave
**Allow List All Bucket Names** disabled. If the key is account-wide or scoped
to another bucket, delete it and create a replacement; Backblaze key scope
cannot be safely corrected by renaming the key. The API needs object
`listFiles`, `readFiles`, `writeFiles`, and `deleteFiles` capabilities; it must
not have `writeBuckets`, `deleteBuckets`, or key-management capabilities.

Before the automated host replacement, create a second key named
`kamori-production-postgres` with access only to
`kamori-production-postgres`, **Read and Write** access, and **Allow List All
Bucket Names** enabled for S3 SDK compatibility. Store its displayed `keyID`
and one-time `applicationKey` in the two encrypted Pulumi settings documented
below. Do not copy either value to a host; role-specific cloud-init writes the
root-owned pgBackRest environment.

Create a third key named `kamori-production-replication` with **Read Only**
access only to `kamori-production-primary` and **Allow List All Bucket Names**
enabled. Store it in the two encrypted Pulumi settings documented below. Do not
reuse the read-write API key for replication. Pulumi delivers this identity
only to the ops backup worker.

Hetzner Object Storage topology is not operator configuration. Pulumi fixes the
reviewed DR location to `fsn1`, derives the endpoint
`fsn1.your-objectstorage.com`, derives the bucket name from the Pulumi stack
(`kamori-app-production-dr` for `production`), and creates the private bucket
through the S3 API. Changing its location or naming policy is a reviewed data
migration in `infra`, not a secret-setting step.

Hetzner does not expose S3 credential creation through its S3 API. The access
and secret keys below are therefore the only manual Object Storage bootstrap
inputs.

### Current operator checklist

While the provisioning change is being reviewed, create and retain the two
Backblaze pairs described above. Do not paste them into chat, GitHub, a server,
or a command argument. After the change is merged, from `infra` run:

```bash
pulumi stack select production
pulumi config set --secret kamori:postgresBackupKeyId
pulumi config set --secret kamori:postgresBackupApplicationKey
pulumi config set --secret kamori:b2ReplicationKeyId
pulumi config set --secret kamori:b2ReplicationApplicationKey
pulumi config
git diff -- Pulumi.production.yaml
```

Each `config set --secret` command opens a hidden prompt. The diff must contain
only `secure:` ciphertext for these four values. Do not change
`hostProvisioningPhase` until the `retire` update has completed successfully.

After the subsequent `replace` update, transfer the generated deployment
outputs directly into the protected GitHub Environment:

```bash
pulumi stack output deploySshPrivateKey --show-secrets \
  | gh secret set BETA_DEPLOY_SSH_PRIVATE_KEY --env production
pulumi stack output sshHostCaPublicKey \
  | gh variable set BETA_SSH_HOST_CA_PUBLIC_KEY --env production
pulumi stack output opsPublicIPv4 \
  | gh variable set BETA_OPS_HOST --env production
gh variable set BETA_APP_ONE_HOST --env production --body 10.42.0.11
gh variable set BETA_APP_TWO_HOST --env production --body 10.42.0.12
```

These commands do not configure a host. They cross the one intentional trust
boundary between the encrypted Pulumi state and the protected deployment
workflow. The app host variables remain `10.42.0.11` and `10.42.0.12`.

### `kamori:hcloudToken` — Hetzner infrastructure API token

- **Classification:** secret provider credential.
- **Value:** a production-project token created in Hetzner Cloud with only the
  permissions required to manage this stack.
- **Purpose:** lets Pulumi create and update networks, firewalls, servers,
  volumes, load balancers, the ACME DNS zone, and the managed TLS certificate.
- **Dependencies:** every Hetzner resource deployment depends on it. It is used
  by Pulumi only and must never be copied to a VM.
- **Loss or rotation:** losing it does not destroy resources, but blocks future
  updates. Create and test a replacement, update this secret, run preview, then
  revoke the old token.

```bash
pulumi config set --secret kamori:hcloudToken
```

### `kamori:porkbunApiKey` — Porkbun DNS API key

Create the credential in Porkbun before running either configuration command:

1. Open **Account → API Access**, name a dedicated key such as
   `kamori-production-pulumi`, and select **Create API Key**.
2. Copy both displayed values into the encrypted recovery record before leaving
   the page. Porkbun shows the secret key only once.
3. Open **Domain Management → kamori.app → Details** and enable **API Access**
   for this domain. Do not enable it on unrelated domains.
4. If the current Porkbun API-key screen offers per-key domain restrictions or
   spending controls, restrict the key to `kamori.app` and set a zero-spend
   policy. DNS reconciliation does not need purchasing permission.

The official Porkbun walkthrough is linked from its
[API documentation](https://kb.porkbun.com/article/190-getting-started-with-the-porkbun-api).

- **Classification:** high-value provider credential identifier stored as a
  secret. It is one half of the Porkbun API credential pair.
- **Value:** the API key from a dedicated Porkbun automation credential whose
  domain access is restricted to `kamori.app`. Enable API access for the domain
  before creating the key. Do not use a personal catch-all key.
- **Purpose:** identifies Pulumi when it creates and updates the public A/AAAA
  records and the twelve NS records that delegate four ACME challenge names to
  Hetzner.
- **Dependencies:** it is useless without `porkbunSecretApiKey`. Porkbun stays
  authoritative for `kamori.app`; this key does not move the registration or
  replace the domain nameservers.
- **Boundary:** do not IP-restrict this key to a developer workstation because
  the protected GitHub runner also needs it through encrypted Pulumi config.
  Use Porkbun's domain restriction and disable spending capabilities instead.
- **Loss or rotation:** losing it blocks DNS reconciliation but does not remove
  existing records. Create a replacement pair, update both Pulumi secrets,
  verify a preview, and only then revoke the old pair.

```bash
pulumi config set --secret kamori:porkbunApiKey
```

Paste the key at the hidden prompt. Never place it after the command because
command-line arguments may be retained in shell history.

### `kamori:porkbunSecretApiKey` — Porkbun DNS API secret

- **Classification:** high-value provider authentication secret.
- **Value:** the secret API key paired with `porkbunApiKey`. Porkbun displays
  this value when the credential is created; preserve an encrypted recovery
  copy because it may not be shown again.
- **Purpose:** proves possession of the Porkbun automation credential and
  authorizes DNS record create, read, update, and delete operations.
- **Dependencies:** the Pulumi Porkbun provider needs both halves. A mismatch
  causes the infrastructure preview/update to fail before DNS changes occur.
- **Compromise:** an attacker with both values can redirect Kamori hostnames or
  break ACME validation within the credential's domain scope. Revoke the pair,
  issue a replacement, restore the declared records with Pulumi, and review the
  Porkbun DNS audit/history immediately.
- **Rotation:** update `porkbunApiKey` and this value together, run a protected
  preview and update, then revoke the previous pair.

```bash
pulumi config set --secret kamori:porkbunSecretApiKey
```

### `kamori:hetznerObjectAccessKey` — DR infrastructure access key

- **Classification:** secret provider credential identifier.
- **Value:** the S3 access key authorized to create and configure only the DR
  bucket.
- **Purpose:** authenticates Pulumi to Hetzner Object Storage.
- **Boundary:** this is an infrastructure credential, not the replication
  worker credential, and must not be placed on runtime hosts.

```bash
pulumi config set --secret kamori:hetznerObjectAccessKey
```

### `kamori:hetznerObjectSecretKey` — DR infrastructure secret key

- **Classification:** high-value secret provider credential.
- **Value:** the S3 secret paired with `hetznerObjectAccessKey`.
- **Purpose:** signs Pulumi requests that manage the DR bucket.
- **Rotation:** replace the access/secret pair together and revoke the old pair
  only after a successful preview.

```bash
pulumi config set --secret kamori:hetznerObjectSecretKey
```

### `kamori:databasePassword` — application PostgreSQL role password

- **Classification:** runtime secret.
- **Value:** a strong random password for the dedicated `kamori_app` role.
- **Purpose:** authenticates both API nodes to PostgreSQL in addition to their
  dedicated client certificate.
- **Dependencies:** the primary bootstrap creates the `kamori_app` role with
  this password. Pulumi derives the complete `KAMORI_DATABASE_URL` from the
  versioned primary address, port, database name, role, and container TLS
  paths; do not assemble or URL-encode that connection string yourself.
- **Rotation:** rotate the database password in a coordinated maintenance
  change: update the role password and this Pulumi secret, then roll both app
  nodes. Do not leave the database and application with incompatible values.

```bash
pulumi config set --secret kamori:databasePassword
```

Generate the value with `openssl rand -base64 48` and paste it at Pulumi's
hidden prompt. Pulumi places the identical value in the root-owned PostgreSQL
environment and the application connection URL. Do not create either file or
copy the generated URL between systems.

Pulumi renders the following shape into `/etc/kamori/cloud.env`; this is an
output, not a configuration value:

```text
postgres://kamori_app:URL_ENCODED_PASSWORD@10.42.0.21:5432/kamori?sslmode=verify-full&sslrootcert=/run/secrets/postgres-ca.crt&sslcert=/run/secrets/postgres-client.crt&sslkey=/run/secrets/postgres-client.key
```

### `kamori:valkeyPassword` — ephemeral state-store password

- **Classification:** runtime secret.
- **Value:** a strong random password dedicated to the production Valkey
  instance.
- **Purpose:** authenticates API nodes to the ephemeral state store used for
  short-lived OPAQUE flows, device-authorization flows, and rate-limit windows.
  PostgreSQL remains authoritative.
- **Dependencies:** Pulumi derives `KAMORI_VALKEY_URL` from the versioned ops
  private address, port, database index, and this password. Do not assemble or
  URL-encode the connection string yourself. Auth handshakes may fail while
  Valkey is unavailable, but durable user data must not depend on it.
- **Rotation:** change `VALKEY_PASSWORD` and this Pulumi secret together, then
  roll both app nodes. Active handshakes and rate-limit windows may be
  discarded.

```bash
pulumi config set --secret kamori:valkeyPassword
```

Generate it with `openssl rand -base64 48` and paste it at Pulumi's hidden
prompt. Pulumi places it in the root-owned ops environment and safely renders
`redis://:URL_ENCODED_PASSWORD@10.42.0.31:6379/0` into the app environment.

### Pulumi-managed private PostgreSQL PKI

These certificates do not come from Hetzner, Porkbun, Caddy, or the public web
TLS certificate. PostgreSQL uses a separate private CA because its endpoint is
a private IP and both application and jobs clients use mTLS identities.

Pulumi generates and retains:

- an ECDSA P-384 root CA valid for ten years;
- a server certificate for `10.42.0.21`;
- a `kamori_app` client certificate for both app nodes;
- a separate `kamori_jobs` client certificate for the ops worker;
- a 30-day early-renewal window for the 397-day leaf certificates.

Private keys remain encrypted in the passphrase-protected Pulumi state and are
written only into the role-specific cloud-init for the host that needs them.
There is no local PKI path, certificate upload, or manual host distribution.
Pulumi state and the passphrase are therefore part of the PKI trust boundary;
compromise requires rotating the complete private trust domain. A managed CA
or KMS/HSM is a post-beta hardening option, not a beta bootstrap dependency.

### `kamori:jwtSecret` — access-token signing key

- **Classification:** high-value runtime secret.
- **Value:** at least 32 random bytes, encoded as a transport-safe string and
  generated independently from every other key.
- **Purpose:** signs and verifies HS256 access and pre-authentication tokens.
- **Dependencies:** every API node must use the same value. Anyone holding it
  can forge accepted tokens.
- **Rotation:** requires a coordinated rollout or explicit multi-key support;
  replacing it immediately invalidates every outstanding signed token.

```bash
openssl rand -base64 32
pulumi config set --secret kamori:jwtSecret
```

Generate first, save the value in the offline recovery record, then paste it at
the second command's hidden prompt. Do not pipe the commands together.

### `kamori:adminTotpKek` — operator TOTP encryption key

- **Classification:** 32-byte runtime key-encryption key.
- **Value:** independent standard-Base64 output from `openssl rand -base64 32`.
- **Purpose:** encrypts administrator TOTP seeds at rest in PostgreSQL. It is
  not a TOTP seed and cannot generate an operator's current one-time code by
  itself without the encrypted database value.
- **Dependencies:** admin login and reauthentication depend on it. Losing it
  makes existing encrypted operator TOTP seeds unreadable.
- **Rotation:** requires an explicit decrypt-and-reencrypt migration; never
  overwrite it directly.

```bash
openssl rand -base64 32
pulumi config set --secret kamori:adminTotpKek
```

### `kamori:authTotpKek` — user TOTP encryption key

- **Classification:** 32-byte runtime key-encryption key.
- **Value:** a new independent Base64 key; never reuse `adminTotpKek`.
- **Purpose:** encrypts ordinary user TOTP seeds at rest.
- **Dependencies:** password login with TOTP, reauthentication, TOTP disable,
  and recovery-code management depend on it.
- **Rotation:** requires decrypting and re-encrypting every affected user seed
  in one planned migration.

```bash
openssl rand -base64 32
pulumi config set --secret kamori:authTotpKek
```

### `kamori:b2RuntimeKeyId` — API blob-store key ID

- **Classification:** runtime credential identifier stored as a secret.
- **Value:** the ID of a B2 application key restricted to
  `kamori-production-primary` and normal object operations.
- **Purpose:** authenticates normal encrypted-blob uploads, reads, and deletes.
- **Boundary:** it must not manage buckets or access PostgreSQL backups. For the
  current deployment, this is the existing Backblaze key named
  `kamori-production` after its bucket scope has been verified.

```bash
pulumi config set --secret kamori:b2RuntimeKeyId
```

### `kamori:b2RuntimeApplicationKey` — API blob-store key secret

- **Classification:** runtime credential secret.
- **Value:** the application key paired with `b2RuntimeKeyId`.
- **Purpose:** signs the API's S3-compatible requests to the primary blob
  bucket.
- **Compromise:** rotate the pair and audit bucket access. E2EE keeps stored
  bytes encrypted, but a stolen credential can still read/delete ciphertext or
  consume quota according to its permissions.

```bash
pulumi config set --secret kamori:b2RuntimeApplicationKey
```

### `kamori:postgresBackupKeyId` — PostgreSQL backup key ID

- **Classification:** bucket-scoped runtime credential identifier stored as a
  secret.
- **Value:** the `keyID` of `kamori-production-postgres`, restricted to the
  private `kamori-production-postgres` bucket with Read and Write access.
- **Purpose:** lets pgBackRest write WAL/full/differential backups, read them
  for restore, and expire retained backup objects.
- **Boundary:** it must not access the primary blob bucket, Pulumi state, the DR
  bucket, bucket administration, or key administration.
- **Delivery:** Pulumi writes it only into the root-owned PostgreSQL
  environment on `db-primary`; it is not a GitHub secret.

```bash
pulumi config set --secret kamori:postgresBackupKeyId
```

### `kamori:postgresBackupApplicationKey` — PostgreSQL backup secret

- **Classification:** high-value backup credential secret.
- **Value:** the one-time `applicationKey` paired with
  `postgresBackupKeyId`.
- **Purpose:** signs pgBackRest's S3-compatible requests.
- **Compromise:** create a replacement bucket-scoped pair, update both Pulumi
  values, apply the stack, confirm the dedicated backup service externally,
  and then revoke the old pair.

```bash
pulumi config set --secret kamori:postgresBackupApplicationKey
```

### `kamori:b2ReplicationKeyId` — DR source read-only key ID

- **Classification:** bucket-scoped runtime credential identifier stored as a
  secret.
- **Value:** the `keyID` of `kamori-production-replication`, restricted to
  `kamori-production-primary` with Read Only access.
- **Purpose:** lets the ops worker copy encrypted application blobs into the
  independent Hetzner DR bucket and verify sampled ciphertext.
- **Boundary:** it cannot upload, overwrite, or delete primary objects and must
  not access PostgreSQL backups or Pulumi state.

```bash
pulumi config set --secret kamori:b2ReplicationKeyId
```

### `kamori:b2ReplicationApplicationKey` — DR source read-only secret

- **Classification:** runtime credential secret.
- **Value:** the one-time `applicationKey` paired with
  `b2ReplicationKeyId`.
- **Delivery:** Pulumi writes it only to the root-owned backup environment on
  `ops`; it is not shared with app or database hosts.
- **Rotation:** replace both replication values together and revoke the old
  pair only after the updated ops host configuration is applied.

```bash
pulumi config set --secret kamori:b2ReplicationApplicationKey
```

### `kamori:metricsBearerToken` — Prometheus scrape credential

- **Classification:** runtime secret.
- **Value:** at least 32 random bytes generated independently.
- **Purpose:** authorizes Prometheus to read the API's aggregate `/metrics`
  endpoint. It does not enable user telemetry or content collection.
- **Dependencies:** the same value must be installed on app nodes and in the
  Prometheus credentials file on ops.
- **Rotation:** update Prometheus and the app nodes in a coordinated change to
  avoid a monitoring blind spot.

```bash
openssl rand -base64 32
pulumi config set --secret kamori:metricsBearerToken
```

Pulumi renders the application values into `/etc/kamori/cloud.env` and the
certificate/key files on both app nodes. Do not create or patch those generated
files manually: unmanaged changes disappear on replacement and can leave the
two nodes cryptographically inconsistent.

Pulumi secret encryption protects the stack file, previews, normal CLI output,
and state backups. It does not hide a value from the cloud provider or the host
that must ultimately receive it: app-node `user_data`, root on the node, and the
running container are inside the deployment trust boundary. This is why the
provider account, Pulumi backend, host access, and GitHub Environment all need
least privilege and audit logging.

Verify that secret values are encrypted:

```bash
pulumi config
pulumi stack export --show-secrets=false
git diff -- Pulumi.production.yaml
```

The stack file must show `secure:` ciphertext, never the pasted value.

## 4. Configure GitHub Actions

In the GitHub repository, open **Settings → Environments → New environment**,
enter `production`, and select **Configure environment**. Then:

1. Under **Deployment branches and tags**, choose **Selected branches and
   tags** and add only `main`. This prevents an unreviewed feature branch from
   receiving production credentials or changing infrastructure.
2. Add required reviewers when a second maintainer exists. Do not allow the
   workflow author to approve their own production deployment. Until a second
   maintainer exists, keep the workflow manual and review every preview before
   running `up`.
3. Add each Environment secret and variable described separately below.
4. In **Settings → Actions → General**, allow only reviewed actions pinned to a
   full commit SHA and disable approval-free workflows from forks.

### `PULUMI_CONFIG_PASSPHRASE` — Pulumi stack decryption secret

- **GitHub location:** `production` Environment secret.
- **Value:** exactly the passphrase chosen for the Pulumi `production` stack.
- **Purpose:** unlocks passphrase-encrypted configuration and encrypted values
  in Pulumi state during `preview` and `up`.
- **Dependencies:** the infrastructure workflow cannot read `hcloudToken`,
  Porkbun credentials, B2 credentials, runtime secrets, or host user-data
  without it.
- **Boundary:** it is not a cloud-provider credential and grants no Hetzner,
  Porkbun, or B2 access by itself. Combined with Pulumi state/config and backend
  access, it exposes every Pulumi-managed secret.
- **Recovery and rotation:** keep an independent offline copy. Rotate with
  `pulumi stack change-secrets-provider passphrase`, then immediately update
  GitHub and the recovery record.

### `B2_PULUMI_KEY_ID` — Pulumi-state B2 key identifier

- **GitHub location:** `production` Environment secret.
- **Value:** the `keyID` of the dedicated B2 Application Key restricted to
  `kamori-production-pulumi-state`.
- **Purpose:** maps to `AWS_ACCESS_KEY_ID` for B2's S3-compatible API and
  identifies the credential used to read and update Pulumi state.
- **Dependencies:** it is required before Pulumi can access the stack; it cannot
  be stored inside that stack's encrypted configuration.
- **Boundary:** the identifier is not sufficient to authenticate by itself,
  but it is kept in the protected Environment alongside its secret half to
  avoid scattering deployment metadata.
- **Rotation:** create a replacement bucket-restricted key, update this value
  and `B2_PULUMI_APPLICATION_KEY` together, verify a preview, then revoke the
  previous key.

### `B2_PULUMI_APPLICATION_KEY` — Pulumi-state B2 secret

- **GitHub location:** `production` Environment secret.
- **Value:** the one-time `applicationKey` shown when the dedicated
  bucket-restricted B2 key is created.
- **Purpose:** maps to `AWS_SECRET_ACCESS_KEY` and authenticates reads, writes,
  history updates, and lock creation/deletion in the Pulumi-state bucket.
- **Dependencies:** GitHub Actions cannot preview or update infrastructure
  without both this value and `B2_PULUMI_KEY_ID`.
- **Boundary:** this key must have access only to
  `kamori-production-pulumi-state`. It must not access primary user blobs,
  PostgreSQL backups, account administration, or application-key management.
  It does not decrypt Pulumi secrets without `PULUMI_CONFIG_PASSPHRASE`.
- **Rotation:** replace both GitHub B2 values atomically, run a preview, and
  revoke the old key only after the replacement has read and locked the stack.

The workflow hard-codes `production` as its stack name and the reviewed B2 URL
as its backend. There is no `PULUMI_STACK` variable and no
`PULUMI_ACCESS_TOKEN`: Kamori does not use Pulumi Cloud.

Provider credentials normally remain encrypted in Pulumi config. The dedicated
B2 state credential is the necessary exception because Pulumi needs it before
it can read and decrypt the stack.

GitHub does not need copies of `jwtSecret`, the OPAQUE setup, TOTP keys,
database credentials, or the API and PostgreSQL-backup object-store keys.
During CD, `PULUMI_CONFIG_PASSPHRASE` unlocks stack-managed secrets in memory
and Pulumi preserves the secret taint through the generated host configuration.

Protect `.github/workflows/`, `infra/`, and `Pulumi.production.yaml` with
CODEOWNERS and branch protection. Production workflows must use only
`contents: read`, must not run through `pull_request_target`, and must not print
the environment or Pulumi secret inputs.

## 5. Preview and deploy

Run a local preview first:

```bash
cd infra
# First load the credentials, passphrase, region, and checksum compatibility
# settings described in section 1.
pulumi stack select production
pulumi preview --diff
```

Then run the `Hosted infrastructure` workflow with `preview`. Inspect the
complete plan and run it again with `up`. App nodes must receive the same OPAQUE
setup file. Sticky sessions are neither required nor accepted as an
authentication correctness mechanism.

Because cloud-init is creation-time configuration, changing a host-delivered
secret is a node-replacement operation, not an in-place file edit. The initial
`replace` phase intentionally recreates all empty beta hosts and is therefore a
bootstrap maintenance window. After the first deployment, never use that phase
as a routine secret-rotation mechanism: implement and exercise a per-node
replacement procedure before rotating a host-delivered secret. Routine
application CD does not rewrite `/etc/kamori/cloud.env` or replace a host.

After the application migration runs, each server records the SHA-256
fingerprint of its setup in `server_security_config`. A node with a different
setup refuses to start.

## 6. Validate the deployment

Complete all checks before enabling registration:

1. Register a disposable account through app node A.
2. Sign in through app node B.
3. Restart node A and sign in again.
4. Restart node B and sign in again.
5. Temporarily supply a different setup in a staging environment and verify
   that the server refuses to start with a fingerprint mismatch.
6. Restore the Pulumi stack and OPAQUE setup from the offline recovery copies in
   an isolated staging project.

Delete the disposable account and recovery material after the exercise.

## Rotation

Do not rotate the OPAQUE setup as routine key hygiene: existing OPAQUE password
files are bound to it. A future setup version is introduced only through an
explicit password-record migration in which users authenticate and create a new
record. Keep every referenced setup version available until no account depends
on it.

The Pulumi passphrase can be rotated independently:

```bash
cd infra
pulumi stack select production
pulumi stack change-secrets-provider passphrase
```

Update `PULUMI_CONFIG_PASSPHRASE` in the GitHub `production` Environment and the
offline recovery record immediately after the migration.

Rotating `refreshRotationKey` requires revoking every refresh session in the
same maintenance window. Rotating either TOTP KEK requires an explicit
decrypt-and-reencrypt migration first. Never replace those values in isolation.

## 7. Add deployment-only GitHub secrets

Runtime secrets remain in Pulumi, but the deployment workflow needs the private
half of the Pulumi-generated deployment identity. Copy it once into the
protected GitHub `production` Environment after the `replace` infrastructure
update. Do not generate or distribute host keys manually.

### `BETA_DEPLOY_SSH_PRIVATE_KEY` — deployment-only SSH identity

- **GitHub location:** `production` Environment secret.
- **Value:** the complete private half of the Pulumi-generated
  `ssh-deploy-key`. Transfer it directly from the encrypted stack output:

  ```bash
  pulumi stack output deploySshPrivateKey --show-secrets \
    | gh secret set BETA_DEPLOY_SSH_PRIVATE_KEY --env production
  ```

- **Purpose:** authenticates a protected GitHub-hosted job through `ops` and
  then as `deploy` to the two private app nodes on TCP `2022`.
- **Dependencies:** Pulumi cloud-init installs the public half, password-locks
  the account, and installs root-owned deployment entrypoints. No SSH session
  or copied repository checkout is used for bootstrap.
- **Boundary:** the private key is materialized only under `RUNNER_TEMP` for the
  deployment step and deleted by a shell trap. The app account is not in the
  Docker group and sudo permits only three fixed root-owned wrappers. The ops
  authorization permits only TCP forwarding to the two private app SSH
  endpoints and forces `/bin/false` for session commands.
- **Rotation:** replace the Pulumi `ssh-deploy-key` resource in a reviewed
  update, transfer the new output to GitHub, deploy once, then remove the old
  GitHub value. Server distribution remains automatic.

### `BETA_SSH_HOST_CA_PUBLIC_KEY` — trusted host authority

- **GitHub location:** `production` Environment variable, not a secret.
- **Value:** the public OpenSSH key of Pulumi's generated host CA. Transfer it
  without ever reading host keys from the network:

  ```bash
  pulumi stack output sshHostCaPublicKey \
    | gh variable set BETA_SSH_HOST_CA_PUBLIC_KEY --env production
  ```

- **Purpose:** authenticates the certified `ops`, `app-1`, and `app-2` host
  identities without `ssh-keyscan` or trust on first use.
- **Rotation:** update the GitHub variable only from the reviewed Pulumi stack
  output. Never replace it with a value learned from the target network.

### GHCR authentication — no persistent secret

The deployment job has `packages: read` and passes its short-lived built-in
`GITHUB_TOKEN` through stdin to the root-owned registry-login wrapper. Do not
create `GHCR_READ_USER`, `GHCR_READ_TOKEN`, a PAT, or a machine account for this
workflow. The token expires with the job; an expired login cannot stop already
running containers and the next job refreshes it before pulling images.

### `BETA_APP_ONE_HOST` — first deployment SSH destination

- **GitHub location:** `production` Environment variable, not a secret.
- **Value:** `10.42.0.11`.
- **Purpose:** tells the GitHub-hosted runner which private host receives the
  migration-first rollout through the ops bastion.
- **Dependencies:** do not append a port; the workflow supplies `2022`.
  Swapping app one and app two changes which node runs database migrations.

### `BETA_APP_TWO_HOST` — second deployment SSH destination

- **GitHub location:** `production` Environment variable, not a secret.
- **Value:** `10.42.0.12`.
- **Purpose:** identifies the second app node in the rolling deployment.
- **Dependencies:** it must be a different private address from app one, and
  the ops firewall/private network must allow the connection on `2022`.

### `BETA_OPS_HOST` — public bastion destination

- **GitHub location:** `production` Environment variable, not a secret.
- **Value:** the stable Pulumi-managed public IPv4 address of `ops`. Transfer it
  after the `replace` update:

  ```bash
  pulumi stack output opsPublicIPv4 \
    | gh variable set BETA_OPS_HOST --env production
  ```

- **Purpose:** gives the GitHub-hosted deployment job its only public SSH
  destination. App and database nodes have no public IP addresses.
- **Boundary:** do not put a username or port in the value; the workflow pins
  user `deploy`, TCP `2022`, and the certified host alias independently.

### Release Environment boundary

Create a second GitHub Environment named `release`, restrict it to protected
version tags (`v*`), and require manual approval. These values are unnecessary
for infrastructure provisioning and should not be added until the corresponding
developer accounts, package identities, and offline recovery process exist.

### `ANDROID_KEYSTORE_BASE64` — Android signing keystore

- **GitHub location:** `release` Environment secret.
- **Value:** the production JKS/PKCS#12 keystore encoded as one-line Base64.
- **Purpose:** supplies the private signing identity for Play, Accrescent, and
  universal Android artifacts.
- **Dependencies:** future updates must use the same signing lineage. Base64 is
  transport encoding, not encryption.
- **Loss:** losing the only signing key may make installed applications
  impossible to update outside store-managed key-upgrade mechanisms.

### `ANDROID_KEYSTORE_PASSWORD` — Android keystore password

- **GitHub location:** `release` Environment secret.
- **Value:** the password protecting the keystore container.
- **Purpose:** allows Gradle to open `ANDROID_KEYSTORE_BASE64` after decoding.
- **Dependencies:** it does not replace the separate key-entry password.

### `ANDROID_KEY_ALIAS` — Android signing entry name

- **GitHub location:** `release` Environment secret because it is credential
  metadata and need not be published.
- **Value:** the exact alias of the production signing key inside the keystore.
- **Purpose:** selects the correct entry when a keystore contains more than one
  certificate.
- **Dependencies:** a wrong alias makes signing fail without damaging the
  keystore.

### `ANDROID_KEY_PASSWORD` — Android private-key password

- **GitHub location:** `release` Environment secret.
- **Value:** the password protecting the selected private-key entry.
- **Purpose:** authorizes Gradle to use the key selected by
  `ANDROID_KEY_ALIAS`.
- **Dependencies:** may equal the keystore password only if that was an
  intentional keystore choice; do not assume they are interchangeable.

### `APPLE_CERTIFICATE` — macOS signing identity archive

- **GitHub location:** `release` Environment secret.
- **Value:** the Developer ID Application PKCS#12 archive encoded as one-line
  Base64.
- **Purpose:** signs the Tauri macOS application distributed outside the Mac
  App Store.
- **Dependencies:** the archive must contain the private key and a currently
  valid certificate for `APPLE_TEAM_ID`.

### `APPLE_CERTIFICATE_PASSWORD` — macOS PKCS#12 password

- **GitHub location:** `release` Environment secret.
- **Value:** the export password chosen when creating `APPLE_CERTIFICATE`.
- **Purpose:** lets the release workflow import the certificate into its
  temporary keychain.

### `APPLE_SIGNING_IDENTITY` — macOS codesign identity selector

- **GitHub location:** `release` Environment secret because it identifies the
  signing account.
- **Value:** the exact codesign identity name or fingerprint expected by the
  workflow.
- **Purpose:** prevents the runner from choosing an unintended certificate when
  more than one identity is imported.

### `APPLE_ID` — App Store Connect account identifier

- **GitHub location:** `release` Environment secret.
- **Value:** the Apple ID used by the release automation account.
- **Purpose:** authenticates notarization and Apple upload tooling together
  with `APPLE_PASSWORD` and `APPLE_TEAM_ID`.
- **Boundary:** prefer a dedicated release identity with the minimum App Store
  Connect role instead of a personal owner account.

### `APPLE_PASSWORD` — Apple app-specific password

- **GitHub location:** `release` Environment secret.
- **Value:** an app-specific password generated by Apple, never the Apple
  account's primary password.
- **Purpose:** authenticates non-interactive notarization/upload commands.
- **Rotation:** revoke only this app-specific password and issue a replacement;
  the signing certificates do not need to change with it.

### `APPLE_TEAM_ID` — Apple developer team identifier

- **GitHub location:** `release` Environment secret because it is account
  metadata used with other credentials.
- **Value:** the exact Team ID owning the macOS and iOS application records.
- **Purpose:** scopes signing, notarization, and iOS export to the correct Apple
  developer team.
- **Dependencies:** shared by the macOS and iOS jobs.

### `IOS_CERTIFICATE` — iOS distribution identity archive

- **GitHub location:** `release` Environment secret.
- **Value:** an Apple Distribution PKCS#12 identity encoded as one-line Base64.
- **Purpose:** signs the iOS archive submitted to TestFlight/App Store Connect.
- **Dependencies:** must belong to `APPLE_TEAM_ID` and match the provisioning
  profile's permitted certificate.

### `IOS_CERTIFICATE_PASSWORD` — iOS PKCS#12 password

- **GitHub location:** `release` Environment secret.
- **Value:** the export password for `IOS_CERTIFICATE`.
- **Purpose:** unlocks the archive during import into the ephemeral CI
  keychain.

### `IOS_PROVISIONING_PROFILE` — iOS App Store profile

- **GitHub location:** `release` Environment secret.
- **Value:** the App Store distribution `.mobileprovision` for application ID
  `app.kamori.mobile`, encoded as one-line Base64.
- **Purpose:** binds the signed build to the bundle ID, team, entitlements, and
  allowed distribution method.
- **Dependencies:** must match the iOS certificate and the application's actual
  entitlements, including any passkey-associated domains added later.

### `IOS_KEYCHAIN_PASSWORD` — ephemeral CI keychain password

- **GitHub location:** `release` Environment secret.
- **Value:** an independent random value generated with
  `openssl rand -base64 32`.
- **Purpose:** protects only the temporary keychain created on the macOS GitHub
  runner while importing the iOS identity.
- **Boundary:** it is not an Apple account or certificate password and may be
  rotated without reissuing the application certificate.

### `WINDOWS_CERTIFICATE` — Windows code-signing archive

- **GitHub location:** `release` Environment secret.
- **Value:** the production PFX encoded as one-line Base64.
- **Purpose:** signs Windows desktop binaries and installers so Windows can
  verify publisher identity and file integrity.
- **Dependencies:** the PFX must contain the private key and a valid code-signing
  certificate chain.

### `WINDOWS_CERTIFICATE_PASSWORD` — Windows PFX password

- **GitHub location:** `release` Environment secret.
- **Value:** the password chosen when exporting `WINDOWS_CERTIFICATE`.
- **Purpose:** allows the workflow to import and use the private signing key.

### `WINDOWS_CERTIFICATE_THUMBPRINT` — Windows identity selector

- **GitHub location:** `release` Environment secret because it identifies the
  production signing identity.
- **Value:** the normalized thumbprint of the certificate inside the PFX.
- **Purpose:** forces the workflow to select the expected certificate rather
  than any other certificate installed on the runner.

### `FLATPAK_GPG_PRIVATE_KEY` — Flatpak repository signing key

- **GitHub location:** `release` Environment secret.
- **Value:** the exported private GPG key encoded as one-line Base64.
- **Purpose:** signs the repository metadata consumed by users of Kamori's own
  Flatpak repository.
- **Dependencies:** clients trust this key across updates. Keep an offline
  primary/recovery copy and document revocation before publishing the repo.

### `FLATPAK_GPG_KEY_ID` — Flatpak signing-key selector

- **GitHub location:** `release` Environment secret because it identifies the
  production signing key.
- **Value:** the full fingerprint or unambiguous key ID corresponding to
  `FLATPAK_GPG_PRIVATE_KEY`.
- **Purpose:** selects the intended GPG key for repository signing and export.

Convert binary signing files to one-line base64 without writing an extra copy:

```bash
base64 < android-release-keystore.jks | tr -d '\n'
base64 < apple-distribution-certificate.p12 | tr -d '\n'
base64 < windows-code-signing.pfx | tr -d '\n'
gpg --batch --export-secret-keys FLATPAK_GPG_KEY_ID | base64 | tr -d '\n'
```

Paste each output into the matching Environment secret. Preserve the original
signing keys, passwords, certificate chains, recovery codes, and revocation
procedure in the offline recovery record. GitHub is a delivery copy, not the
only copy.

Before the first release, trigger the workflow manually for a protected test
tag. Verify signatures on every produced artifact on a clean machine, install
the Flatpak from the generated signed repository, install the Accrescent APK
set, and record the checksums and certificate fingerprints in the release
record. Delete the test release after the exercise; do not delete or regenerate
the signing identities.
