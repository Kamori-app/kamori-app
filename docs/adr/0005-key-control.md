# ADR 0005: Durable content keys and HPKE control

Status: accepted

## Decision

Durable operations and snapshots use retained per-space content-key epochs.
MVP key distribution wraps epochs to devices with standard HPKE. Key-control
messages are versioned so MLS may be adopted later without using MLS application
messages as the durable archive. Membership-changing rotations atomically bind
a stable request id and base cursor to complete remaining-device packages,
remaining-member recovery packages, encrypted metadata, and signed snapshot-v2
coverage. New invitations are prepared by such a rotation and expose only the
new current-state epoch.

## Consequences

Member removal rotates future data without pretending to erase previously
decrypted history. Snapshot v2 preserves all materialized conflict branches;
membership history entitlement remains distinct from the current-state start
cursor used by a new device. The MVP retains operations and does not perform
history-erasing compaction. MLS is a future control-plane option, not a stored-
data format.
