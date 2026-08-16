# ADR 0002: Security spaces

Status: accepted

## Decision

The smallest independently shared unit is a security space. Membership, role,
key epoch, log cursor, quota ownership, and authorization are scoped to it.
Workspaces are higher-level organization containers.

## Consequences

Sharing one collection never grants access to other workspace ciphertext or
metadata. Future document spaces use the same boundary. Workspace-wide crypto
groups are insufficient.
