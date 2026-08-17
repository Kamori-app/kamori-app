# ADR 0003: Signed encrypted operation log

Status: accepted

## Decision

Clients append canonical, device-signed encrypted envelopes with random stream
and operation identifiers. The server verifies admission and assigns an atomic
monotonic per-space sequence. MVP PIM payloads use versioned domain operations;
future document CRDT changes are separate codecs.

## Consequences

Retries are idempotent, server sequence is only a transport cursor, semantic
metadata remains encrypted, and the storage layer is not coupled to DAV or a
specific CRDT.
