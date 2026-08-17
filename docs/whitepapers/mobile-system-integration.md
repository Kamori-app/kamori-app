# Kamori mobile system integration

Status: MVP product and privacy contract

[Русская версия](mobile-system-integration.ru.md)

## The short version

Kamori provides its own calendar, task, and contact screens on Android and iOS.
The MVP field set is intentionally small and documented in `SPEC.md`. You do
not need to install a profile, configure a DAV account, keep the app open, or
connect another app to `127.0.0.1`.

If you want Kamori data to appear in the phone's built-in Calendar or Contacts
apps, Kamori can create an optional system projection. The app asks first. You
can decline and continue using Kamori normally.

## Why Kamori does not run a local server on phones

Mobile operating systems suspend background applications. A localhost DAV
server can therefore disappear exactly when Calendar or Contacts tries to use
it. Keeping such a server alive also consumes battery and creates confusing
network and credential setup.

Kamori instead uses the operating system's supported data APIs and scheduled
background work:

- Android uses the Calendar and Contacts provider APIs and scheduled
  WorkManager synchronization.
- iOS uses EventKit and Contacts after the user grants access.
- Kamori's encrypted operation log remains the source of truth. System stores
  are projections, not an encryption or synchronization backend.

This is more reliable, needs no localhost password, and matches the security
model of each platform.

## What happens when integration is enabled

1. Kamori explains which system data category it wants to use.
2. Android or iOS shows its own permission prompt.
3. Kamori creates a clearly named Kamori calendar and individually tracked
   contact copies.
4. Remote changes are decrypted on the device and projected to the enabled
   system store.
5. Changes made in Kamori still become signed encrypted operations and enter
   the durable local outbox before upload.

Kamori never sends plaintext calendar entries, tasks, or contacts to the
Kamori service.

## Your choices

You can independently choose whether to expose calendars and contacts to the
system. A denial does not block login, offline use, sharing, export, or the
native Kamori interface. Permissions can be changed later in Kamori settings
or the operating system's settings.

Tasks remain inside Kamori for the MVP because Android and iOS do not offer one
shared system reminders interface with equivalent behavior.

Disabling integration stops future projection work. Kamori then offers a clear
choice to keep or remove the already projected system data; it does not delete
system data silently.

## Important privacy boundary

End-to-end encryption protects data while it is stored by or transported
through Kamori services. Once you choose system integration, decrypted copies
exist in the phone's Calendar or Contacts database. They are then subject to
the device lock, operating-system backups, other apps with permission, and any
separate accounts configured in those system apps.

For the strongest isolation, use the Kamori app without system projection.

## Editing and deletion in the MVP

The first release intentionally uses a one-way Kamori-to-system projection.
Edit encrypted source data in Kamori. Direct edits to a projected Calendar or
Contacts copy are not imported and can be replaced by the next projection.
This keeps the encrypted operation log authoritative and avoids silently
turning unrelated provider changes into Kamori writes. Bidirectional provider
import remains a post-MVP roadmap item and requires explicit conflict UX.

Kamori assigns stable identifiers to projected objects and avoids duplicating
them across sync runs. Deletion in Kamori creates a tombstone operation and
removes the tracked system copy. Offline deletion remains in the local outbox
until acknowledged. Removing the app or revoking permission does not pretend
that unsynchronized edits were uploaded.

## DAV remains available on desktop

Desktop Kamori can expose an authenticated localhost CalDAV/CardDAV bridge for
legacy desktop applications. That bridge is an adapter over the same canonical
encrypted data and uses a separate random DAV credential. It is intentionally
not part of the Android or iOS architecture.
