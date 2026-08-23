# ADR 0004: Identity, devices, and recovery

Status: accepted

## Decision

Registration is web-only and uses a canonical username. Authentication uses
OPAQUE/password, web/desktop passkeys, and optional TOTP; native mobile
passkeys are deferred. Every Kamori device has independent signing and
key-agreement keys. A successful login issues a short-lived enrollment grant
that binds idempotently to one exact device request. The account master key
deterministically derives a separate recovery HPKE identity, so an authenticated
client can unwrap only its current recovery packages and create packages for
the newly registered device. The 24-word data recovery kit can reset
credentials, recover current space keys, revoke old devices, and bootstrap a
replacement device.

Refresh rotation uses a random per-token-generation request identity persisted
beside the token before network I/O. The server retains the exact-retry mapping
while the replacement session is active; any different reuse identity revokes
the account's refresh sessions. Browser refresh rotates the CSRF generation at
the same boundary.

## Consequences

Authentication does not make the server capable of releasing plaintext
content keys: recovery packages remain client-decrypted. Enrollment tokens
cannot authorize a different device request. There is no email-only reset in
the MVP. Without the recovery kit/account master key or an already provisioned
device, encrypted content cannot be restored. Support has no recovery bypass.
