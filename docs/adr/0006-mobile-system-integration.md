# ADR 0006: Mobile system integration

Status: accepted

## Decision

Android integrates through platform accounts, Calendar/Contacts providers, sync
adapters, and scheduled work. iOS integrates through EventKit/Contacts and a
documented container/profile flow where necessary. Both integrations are
explicitly enabled per collection.

Mobile applications do not run a localhost CalDAV/CardDAV server.

## Consequences

Kamori remains useful without system permissions. Exported system data is
outside Kamori's encrypted local store and has separately documented privacy
limits. Desktop remains the DAV bridge platform.
