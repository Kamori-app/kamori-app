# Production secrets

Kamori stores deployment-owned secret values in Pulumi encrypted stack
configuration. GitHub Actions stores only the credentials needed to unlock the
Pulumi stack and reach the infrastructure providers. User keys, device keys,
refresh tokens, and security-space keys never belong in Pulumi or GitHub.

## Security boundary

- `Pulumi.production.yaml` contains ciphertext under `secure:` keys and may be
  committed.
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

Install the pinned Pulumi and Go versions, authenticate to the Pulumi backend,
then create a passphrase-encrypted stack:

```bash
cd infra
pulumi stack init production --secrets-provider=passphrase
```

Enter a new high-entropy passphrase when prompted. Save it in the organisation's
offline password-manager or recovery record before continuing. Losing both the
passphrase and the recovery copy makes the encrypted stack configuration
unrecoverable.

If the stack already exists with another provider, migrate it instead:

```bash
cd infra
pulumi stack select production
pulumi stack change-secrets-provider passphrase
```

## 2. Generate the OPAQUE server setup

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

Set non-secret topology values with normal Pulumi config and every credential
with the hidden `--secret` prompt:

```bash
cd infra
pulumi config set kamori:sshKeys operator-key
pulumi config set kamori:adminCidrs 203.0.113.10/32
pulumi config set kamori:tlsCertificateIds 123456
pulumi config set kamori:b2Endpoint s3.eu-central-003.backblazeb2.com
pulumi config set kamori:b2Region eu-central-003
pulumi config set kamori:b2Bucket kamori-production-primary
pulumi config set kamori:b2PostgresBackupBucket kamori-production-postgres
pulumi config set kamori:hetznerObjectEndpoint fsn1.your-objectstorage.com
pulumi config set kamori:hetznerObjectRegion fsn1
pulumi config set kamori:hetznerObjectBucket kamori-production-dr
pulumi config set --secret kamori:hcloudToken
pulumi config set --secret kamori:b2InfraKeyId
pulumi config set --secret kamori:b2InfraApplicationKey
pulumi config set --secret kamori:hetznerObjectAccessKey
pulumi config set --secret kamori:hetznerObjectSecretKey
pulumi config set --secret kamori:databaseUrl
pulumi config set --secret kamori:valkeyUrl
pulumi config set --secret kamori:postgresCaCertificate
pulumi config set --secret kamori:postgresClientCertificate
pulumi config set --secret kamori:postgresClientKey
pulumi config set --secret kamori:jwtSecret
pulumi config set --secret kamori:adminTotpKek
pulumi config set --secret kamori:authTotpKek
pulumi config set --secret kamori:b2RuntimeKeyId
pulumi config set --secret kamori:b2RuntimeApplicationKey
pulumi config set --secret kamori:metricsBearerToken
```

Use separate least-privilege runtime B2 keys for the cloud server. Infrastructure
bucket-management credentials must never be copied to an app host.

Generate `jwtSecret` and `metricsBearerToken` with at least 32 random bytes.
Generate `adminTotpKek` and `authTotpKek` independently with
`openssl rand -base64 32`. `databaseUrl` must require TLS and use a dedicated
application role; `valkeyUrl` must contain the dedicated Valkey password. Pulumi
renders these values into `/etc/kamori/cloud.env` on both app nodes with mode
`0400`; do not create or patch that file manually.

Paste the complete PEM blocks for the PostgreSQL CA, app-client certificate,
and app-client private key into their three hidden Pulumi prompts. The client
certificate must authenticate only the `kamori_app` role, and the server
certificate must contain the database private IP in its SAN because the API
uses `sslmode=verify-full`. Pulumi writes the private key with mode `0400` and
mounts all three files read-only into the API container.

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
   tags** and add only `main`.
2. Add required reviewers when a second maintainer exists. Do not allow the
   workflow author to approve their own production deployment.
3. Under **Environment secrets**, add `PULUMI_CONFIG_PASSPHRASE` and
   `PULUMI_ACCESS_TOKEN` using the values described below.
4. Under **Environment variables**, add `PULUMI_STACK` with the fully qualified
   stack name shown by `pulumi stack --show-name` (for example,
   `organisation/kamori-hosted/production`).
5. In **Settings → Actions → General**, allow only reviewed actions pinned to a
   full commit SHA and disable approval-free workflows from forks.

Add these Environment secrets:

- `PULUMI_CONFIG_PASSPHRASE`: the production stack passphrase.
- `PULUMI_ACCESS_TOKEN`: a token restricted to the Kamori production stack.

Provider credentials normally remain encrypted in Pulumi config. Add a provider
credential directly to GitHub only when the deployment tool requires it before
Pulumi can read the stack.

GitHub does not need copies of `jwtSecret`, the OPAQUE setup, TOTP keys,
database credentials, or object-store application keys. During CD,
`PULUMI_CONFIG_PASSPHRASE` unlocks them in memory and Pulumi preserves the
secret taint through the generated host configuration.

Protect `.github/workflows/`, `infra/`, and `Pulumi.production.yaml` with
CODEOWNERS and branch protection. Production workflows must use only
`contents: read`, must not run through `pull_request_target`, and must not print
the environment or Pulumi secret inputs.

## 5. Preview and deploy

Run a local preview first:

```bash
cd infra
pulumi stack select production
pulumi preview --diff
```

Then run the `Hosted infrastructure` workflow with `preview`. Inspect the
complete plan and run it again with `up`. App nodes must receive the same OPAQUE
setup file. Sticky sessions are neither required nor accepted as an
authentication correctness mechanism.

Because cloud-init is creation-time configuration, changing a host-delivered
secret is a rolling node-replacement operation, not an in-place file edit.
Preview must show one app node at a time, with the other node healthy behind the
load balancer. Temporarily remove Pulumi and Hetzner protection only for the
single reviewed replacement, restore protection immediately, and never replace
both app nodes in one update. Routine application CD does not rewrite
`/etc/kamori/cloud.env`.

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

Runtime secrets remain in Pulumi, but the workflows also need credentials for
SSH deployment and signed client artifacts. Keep these in GitHub Environments;
they are not application configuration and must not be copied into Pulumi.

In the `production` Environment, add:

- `BETA_SSH_KNOWN_HOSTS`: the pinned host-key lines for both app nodes. Obtain
  the fingerprints from the Hetzner console or another authenticated channel,
  compare them with `ssh-keygen -lf`, and only then copy the complete
  `known_hosts` lines. Do not trust an unauthenticated `ssh-keyscan` result.
- `GHCR_READ_USER`: a dedicated machine user used by app nodes.
- `GHCR_READ_TOKEN`: a fine-grained, expiring token for that user with read-only
  access to the four Kamori container packages.

Add the non-secret `BETA_APP_ONE_HOST` and `BETA_APP_TWO_HOST` Environment
variables. Each value is an SSH destination understood by the self-hosted ops
runner, for example `deploy@10.42.0.11`. Verify that the runner's SSH key grants
only the documented deployment commands through `sudo`.

Create a second GitHub Environment named `release`, restrict it to protected
version tags (`v*`), and require manual approval. Add only the signing values
used by `.github/workflows/release-clients.yml`:

- Android: `ANDROID_KEYSTORE_BASE64`, `ANDROID_KEYSTORE_PASSWORD`,
  `ANDROID_KEY_ALIAS`, and `ANDROID_KEY_PASSWORD`.
- macOS desktop: `APPLE_CERTIFICATE`, `APPLE_CERTIFICATE_PASSWORD`,
  `APPLE_SIGNING_IDENTITY`, `APPLE_ID`, `APPLE_PASSWORD`, and `APPLE_TEAM_ID`.
- iOS: `IOS_CERTIFICATE`, `IOS_CERTIFICATE_PASSWORD`,
  `IOS_PROVISIONING_PROFILE`, and `IOS_KEYCHAIN_PASSWORD`. The certificate must
  be an Apple Distribution PKCS#12 identity and the profile must be an App
  Store distribution profile for `app.kamori.mobile`; `APPLE_TEAM_ID` is shared
  with the macOS job.
- Windows desktop: `WINDOWS_CERTIFICATE`, `WINDOWS_CERTIFICATE_PASSWORD`, and
  `WINDOWS_CERTIFICATE_THUMBPRINT`.
- Linux/Flatpak: `FLATPAK_GPG_PRIVATE_KEY` and `FLATPAK_GPG_KEY_ID`.

Convert binary signing files to one-line base64 without writing an extra copy:

```bash
base64 < android-release-keystore.jks | tr -d '\n'
base64 < apple-distribution-certificate.p12 | tr -d '\n'
base64 < windows-code-signing.pfx | tr -d '\n'
gpg --batch --export-secret-keys FLATPAK_GPG_KEY_ID | base64 | tr -d '\n'
```

Paste each output into the matching Environment secret. Use an app-specific
Apple password, not the Apple account password. Preserve the original signing
keys, passwords, certificate chains, recovery codes, and revocation procedure
in the offline recovery record. GitHub is a delivery copy, not the only copy.
Generate `IOS_KEYCHAIN_PASSWORD` independently with `openssl rand -base64 32`;
it protects only the ephemeral CI keychain and is not a certificate password.

Before the first release, trigger the workflow manually for a protected test
tag. Verify signatures on every produced artifact on a clean machine, install
the Flatpak from the generated signed repository, install the Accrescent APK
set, and record the checksums and certificate fingerprints in the release
record. Delete the test release after the exercise; do not delete or regenerate
the signing identities.
