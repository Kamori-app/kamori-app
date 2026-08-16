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

After migrations, run this only on a trusted ops host:

```bash
cloud-server admin-bootstrap <username>
```

The command prints a 15-minute one-time enrollment token and a TOTP secret.
Store TOTP in a separate authenticator, open the console, expand first-time
enrollment, and register a roaming security key. Never put bootstrap output in
shell history, CI logs, tickets, or chat.

The beta verifier uses direct attestation and the current strict Yubico CA
catalog. This is an explicit allowlist. After first sign-in, enroll and test a
second roaming key stored in a separate location; opening registration is
rejected while the acting operator has fewer than two keys.

Every mutation requires another security-key assertion, current TOTP, a reason,
and exact typed confirmation. Operator session and reauthentication tokens are
memory-only. TOTP seeds are encrypted at rest with the independent
`KAMORI_ADMIN_TOTP_KEK`; losing that deployment secret locks operator TOTP
authentication.
