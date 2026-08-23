# Kamori roadmap

Roadmap milestones use exit criteria rather than promised dates.

## MVP: encrypted PIM platform

Exit criteria:

- normative protocol, test vectors, new schema, signed idempotent oplog;
- OPAQUE authentication, web/desktop passkeys, devices, recovery, sessions,
  and key epochs;
- offline web/mobile calendars, tasks, contacts, sharing, and conflicts;
- desktop CalDAV/CardDAV compatibility matrix;
- Android/iOS opt-in system projection without mobile localhost DAV;
- admin console, quotas, waitlist, observability, backup/restore runbooks;
- reproducible platform artifacts and strict GitHub Actions verification;
- honest EN/RU product, user, security, and privacy documentation.

## Post-MVP 1: hardened self-hosting

Entry: hosted MVP protocol is stable.
Exit: documented Compose package, upgrades/backups, operator configuration,
portable storage/email/push providers, and conformance tests.

Client hardening in this milestone includes:

- native Android Credential Manager and iOS AuthenticationServices passkeys,
  enabled only after associated-domain configuration and physical-device
  ceremony/recovery tests pass;
- resumable, paginated DAV collection hydration with progress and cancellation;
  a partially hydrated collection must remain hidden from DAV clients until
  its initial projection is complete.

## Post-MVP 2: realtime documents

Entry: operation transport and key control are stable under production load.
Exit: benchmarked Automerge or alternative codec, encrypted block-document
prototype, multi-user realtime UX, snapshots/history policy, and mobile memory
validation.

Before hosted limits are raised for large or hot spaces, replace the current
atomic all-stream epoch rotation with an authenticated staged protocol:
expiring rotation intents, bounded retry-safe snapshot/package batches,
manifest coverage verification, an atomic epoch commit, and garbage collection
for abandoned pre-commit data. The current atomic request remains the safe MVP
boundary.

Advanced projects/kanban, document audit history UI, and larger attachments are
considered here only after core document behavior is sound.

## Post-MVP 3: B2B office suite

Entry: document collaboration demonstrates product demand.
Exit: organizations, admin roles, SSO/SCIM, enterprise policy/audit exports,
commercial plans, compliance roadmap, and independently deployable proprietary
services where appropriate. A paid independent security audit is scheduled
only after product revenue can sustainably fund it; no earlier milestone may
claim that such an audit has occurred.

Team workspaces require a short-lived, role-bound workspace invitation flow
before first-party clients expose team creation. Security-space invite codes
must continue to grant only the named space; direct user-id admission is not a
substitute for the workspace flow.

## Post-MVP 4: federation

Entry: self-host and B2B identity/key semantics are stable.
Exit: threat-modeled cross-server discovery, trust, key control, abuse handling,
and interoperable conformance suite.

## Explicitly deferred

- outbound CalDAV scheduling/iMIP;
- direct Google/iCloud OAuth migration;
- full edit-history UI for shared collections;
- device-key proof on high-value API calls after a dedicated recovery/automation threat model;
- history-erasing compaction only through a versioned document-generation reset;
- MLS adoption before a measured need and separate control-plane design;
- Flathub submission while its policy excludes automation-assisted projects
  of this kind.
