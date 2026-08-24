# Kamori Operator Console

This is a separately built operator application for `admin.kamori.app`. It does
not reuse consumer identities or sessions and cannot read encrypted content,
content keys, or impersonate users.

## Local development

```bash
bun --filter admin-frontend dev
```

Set `VITE_KAMORI_API_BASE_URL` to the cloud API origin. The cloud server must
set `KAMORI_ADMIN_WEBAUTHN_RP_ORIGIN` to the exact console origin.

## First operator

After migrations, run this only inside the cloud container on a trusted app
host:

```bash
cloud-server admin-bootstrap <username>
```

The command prints a 15-minute one-time enrollment token and a TOTP secret.
Store TOTP in a separate authenticator, open the console, expand first-time
enrollment, and register a passkey. Never put bootstrap output in shell history,
CI logs, tickets, or chat. The browser presents the available authenticators and
the operator chooses between a password manager, a platform passkey, a physical
security key, or another WebAuthn provider. Kamori does not request vendor
attestation or force an authenticator attachment; WebAuthn user verification is
still required.

After first sign-in, a second passkey from an independent authenticator or
provider is recommended for recovery, but it is optional and does not block
opening registration. A second credential in the same synchronized vault does
not protect against losing access to that vault. The console allows existing
passkeys to be renamed; names are unique per operator and every rename is
audited. The final passkey cannot be removed because doing so would lock the
operator out.

Every mutation requires another passkey assertion, current TOTP, a reason,
and exact typed confirmation. Operator session and reauthentication tokens are
memory-only. TOTP seeds are encrypted at rest with the independent
`KAMORI_ADMIN_TOTP_KEK`; losing that deployment secret locks operator TOTP
authentication.
