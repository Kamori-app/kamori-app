# ADR 0001: Product and adapter boundaries

Status: accepted

## Decision

Kamori is an E2EE operation platform. Calendars, tasks, and contacts are the MVP
typed projections. DAV is a desktop-local adapter over materialized state, not
the cloud schema. Web and mobile are first-party PIM clients.

## Consequences

The backend and wire protocol cannot expose DAV-specific event types. PIM and
future document codecs share transport without sharing a merge engine. Mobile
does not carry a localhost DAV server.
