# ADR 0005: Durable content keys and HPKE control

Status: accepted

## Decision

Durable operations and snapshots use retained per-space content-key epochs.
MVP key distribution wraps epochs to devices with standard HPKE. Key-control
messages are versioned so MLS may be adopted later without using MLS application
messages as the durable archive.

## Consequences

Member removal rotates future data without pretending to erase previously
decrypted history. The MVP retains operations and has not yet shipped snapshot
generation or history compaction. Snapshot-based history handoff is a required
future codec capability. MLS is a future control-plane option, not a stored-data
format.
