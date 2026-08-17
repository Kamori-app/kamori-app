# Kamori security and privacy, in plain language

Status: MVP user contract

[Русская версия](security-and-privacy.ru.md)

## The promise

Kamori encrypts calendar events, tasks, contacts, and attachments on your
device before they are uploaded. The hosted service stores ciphertext and the
signed operation envelopes needed to synchronize it. It does not receive the
content keys required to read that material.

This is end-to-end encryption, not anonymity. The service must still know that
an account exists, which opaque spaces and devices belong to it, when encrypted
operations arrive, their padded size, and how much storage or traffic is used.
Network providers can also see that a device connects to Kamori. Titles,
descriptions, contact fields, DAV resource types, and plaintext attachment
names are not intentionally exposed to the service.

## Passwords, devices, and keys

Password authentication uses OPAQUE. The server verifies a login without
storing a password-equivalent database entry that can be used for an offline
guessing attack in the same way as a conventional password hash.

Every approved device has its own signing and encryption keys. Its signature
lets other clients reject operations that were not admitted from an authorized
writer. Removing a device prevents future writes and future key delivery; it
cannot erase plaintext that the device already saw.

The web vault encrypts keys and offline content in IndexedDB. Desktop and
mobile caches use SQLCipher, with the database key protected by the operating
system. These controls reduce exposure from copied application files, but they
cannot protect an unlocked device that is already controlled by malware or an
attacker.

## Recovery is your responsibility

Registration creates a 24-word data recovery kit. Store it offline, separately
from the device and password manager used for daily access. Kamori support
cannot reconstruct it and cannot decrypt your data without it.

Recovery replaces the password record, disables TOTP, revokes existing
sessions, passkeys, and devices, and creates a clean device identity. A current
recovery-wrapped copy of each space key makes the data readable again. If the
kit and every approved device are lost, encrypted data is intentionally
unrecoverable.

TOTP backup codes are different: they help complete a normal sign-in but do not
contain data keys and cannot replace the 24-word recovery kit.

## Sharing

Sharing uses a single-use code that expires after a user-selected period from
15 minutes to 7 days. The code delivers a space key and reader/editor role. A
reader can decrypt and copy content; “read only” means the server will not
accept that member's writes, not that plaintext can be made impossible to copy.

Removing a member rotates the key for future operations. It does not make
previously decrypted information disappear from that person's devices or
backups.

## Optional plaintext projections

Kamori's own clients keep the canonical data inside the encrypted model. Two
optional adapters deliberately cross that boundary:

- the desktop DAV bridge decrypts into a local SQLCipher cache and lets a
  chosen calendar or contacts application access it through authenticated
  loopback DAV;
- Android/iOS system projection writes selected decrypted copies to the
  operating system's Calendar or Contacts database after explicit permission.

Those external applications or system stores may create plaintext indexes,
backups, notifications, or cloud copies under their own policies. Leave the
adapter disabled for the strongest isolation. Mobile never runs a localhost
DAV server.

## Telemetry and operations

Product telemetry, crash reporting, and marketing consent are separate and off
until the user explicitly enables each category. Essential server metrics are
limited to aggregate operational counters and do not include content, keys,
tokens, usernames, or per-space labels.

Quotas and emergency traffic breakers can reject new blob transfers while
authentication, encrypted operation sync, recovery, deletion, and
administration stay available. Kamori does not silently turn off the whole
service to control a bandwidth bill.

## What Kamori does not claim

- no anonymity or traffic-analysis resistance;
- no protection from a compromised, unlocked endpoint;
- no retroactive erasure from former members;
- no completed independent security audit until one is actually commissioned;
- no compatibility guarantee for an untested DAV client;
- no recovery bypass held by the operator.

Security issues should be reported through the private process in
`SECURITY.md`, never through a public issue containing exploit details.
