# ADR 0006: Mobile system integration

Status: accepted

## Decision

Android integrates through Calendar/Contacts provider APIs and scheduled
background work. iOS integrates through EventKit/Contacts. The application
maintains stable per-account mappings for objects it created; both integrations
are one-way from Kamori and explicitly enabled per security space.

Mobile applications do not run a localhost CalDAV/CardDAV server.

## Consequences

Kamori remains useful without system permissions. Exported system data is
outside Kamori's encrypted local store and has separately documented privacy
limits. Desktop remains the DAV bridge platform.
