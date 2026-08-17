# ADR 0004: Identity, devices, and recovery

Status: accepted

## Decision

Registration is web-only and uses a canonical username. Authentication uses
OPAQUE/password, web/desktop passkeys, and optional TOTP; native mobile
passkeys are deferred. Every Kamori device has independent signing and
key-agreement keys. New devices require trusted-device approval. The 24-word
data recovery kit can reset credentials, recover current space keys, revoke old
devices, and bootstrap a replacement device.

## Consequences

Authentication does not automatically release content keys. There is no
email-only reset in the MVP. Without a trusted device or recovery kit encrypted
content cannot be restored. Support has no recovery bypass.
