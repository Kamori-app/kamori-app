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
The current PIM schema uses typed temporal and multi-value fields. Its shared
lossless parser/materializer updates explicitly managed iCalendar/vCard
properties while preserving unknown properties and recurrence exceptions.
Advanced recurrence authoring and a CRDT document codec are roadmap work, not
implicit core capabilities.

The stable PIM operation type is intentionally single-parent. Its current field
schema is v2 and legacy schema-1 operations remain readable. Snapshot v2 checkpoints all
materialized branches in a stream. A key epoch exposes both a membership
history boundary and a current-state recovery cursor, so a new device can load
verified current snapshots without receiving superseded epoch keys. Rotation
persists that cursor with the space, so catch-up cost does not grow with the
operation-log history. It is an idempotent atomic server transaction over the
base cursor, remaining key packages, encrypted metadata, snapshot coverage,
and any deliberately quarantined streams.

See the ADR index for accepted boundaries and consequences.
