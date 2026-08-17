# Architecture overview

Kamori separates its durable encrypted data plane from authentication and key
control.

```text
first-party clients / desktop DAV adapter
                |
      signed encrypted envelopes
                |
          stateless Axum API
          |                    |
      PostgreSQL          encrypted blobs
  auth/authz/oplog        B2 + DR copy
```

The canonical hierarchy is account, workspace, security space, stream, then
operation or snapshot. Workspaces organize data. Security spaces independently
define membership, role, quota ownership, and content-key epochs.

The server authenticates devices, verifies signatures, enforces admission and
quota policy, allocates monotonic per-space cursors, and stores ciphertext. It
does not parse PIM or future document operations.

Clients materialize state locally. The shared Rust core owns cryptography,
envelope encoding, key lifecycle, synchronization, the current PIM codec,
conflict-copy detection, and DAV projection. Platform applications supply
secure storage, networking, lifecycle integration, system PIM adapters, and UI.
Rich recurrence and a CRDT document codec are roadmap work, not implicit core
capabilities.

See the ADR index for accepted boundaries and consequences.
