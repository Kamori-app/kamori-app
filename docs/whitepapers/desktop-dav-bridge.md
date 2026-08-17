# Using the Kamori desktop DAV bridge

Status: MVP user and security contract

[Русская версия](desktop-dav-bridge.ru.md)

## What it is

Kamori's desktop bridge lets a compatible calendar or contacts application
work with a local copy of your Kamori data. It is an adapter for existing apps,
not the foundation of Kamori sync. The signed encrypted operation log remains
the source of truth.

The bridge listens only on `127.0.0.1`. That address means “this computer”. It
is not reachable from another device on your Wi-Fi and it is never exposed as
a public cloud DAV endpoint.

You do not need the bridge to use the native Kamori clients. Leave it stopped
unless another desktop application needs CalDAV or CardDAV.

## The simple setup

1. Sign in to the Kamori desktop application.
2. Create or join at least one space.
3. Open **Dashboard** and choose **Start DAV Bridge**.
4. Choose **Show DAV Setup**. Kamori reveals the setup details for 60 seconds.
5. In your calendar or contacts application, choose the option for a custom or
   advanced CalDAV/CardDAV account.
6. Copy the direct Calendar URL or Address Book URL for the space you want.
7. Use the displayed DAV username and the dedicated DAV password.

Passkey sign-in opens `app.kamori.app` in your normal browser and shows the
same short code in both places. Approve only when the codes match. Session
tokens are delivered through the expiring device-authorization channel, not
through the browser URL. Unlock a new desktop once with the account password
so its operating-system keychain has the account key; later sign-ins can use
the browser flow.

Do not enter your Kamori account password into a DAV client. The random DAV
password exists only for the local bridge and is stored in the operating
system's secure credential store.

Some clients insist on automatic server discovery instead of accepting a
direct collection URL. Those clients are outside the MVP compatibility set.
Kamori will publish a tested client matrix before the public beta and will not
claim compatibility with an untested client.

## What stays encrypted

Kamori downloads encrypted operations and decrypts them on this computer. The
cloud service never receives DAV passwords or plaintext DAV resources.

The local materialized cache is protected with SQLCipher and a key kept in the
operating system's secure credential store. A DAV client may create its own
plaintext database, search index, backup, notification, or export. That copy
is controlled by the DAV client and your operating system, not by Kamori's
end-to-end encryption.

For the strongest isolation, use the native Kamori interface and do not connect
a third-party DAV client.

## Starting, stopping, and background sync

Starting the bridge also starts a periodic encrypted sync loop. **Sync Now**
runs an immediate cycle. Stopping the bridge stops both the localhost listener
and that background loop; the encrypted local cache remains available for the
next start.

If you configure Kamori to hide or minimize on close, the bridge keeps running.
Choosing the normal quit behavior exits the process and therefore stops it.

## Password rotation

If a DAV password was exposed or copied to the wrong application:

1. Stop the bridge.
2. Open **Show DAV Setup**.
3. Choose **Rotate Password** and confirm.
4. Update every DAV client that should retain access.
5. Start the bridge again.

The old password stops working as soon as the bridge is restarted. Rotation
does not change your Kamori account password, encryption keys, or recovery kit.

## Safe removal

Remove the CalDAV/CardDAV account from the third-party application first if you
also want that application's plaintext cache removed. Then stop the bridge.
Logging out of Kamori stops the bridge, locks local account state, and attempts
to revoke the current refresh session on the Kamori service.

Deleting a third-party DAV account does not delete the canonical Kamori space.
Deleting an individual resource while the bridge is connected does create a
signed Kamori tombstone after the client's revision check succeeds.

## Current compatibility boundary

The MVP supports authenticated direct collection URLs, discovery within that
collection, reads, conditional writes, and conditional deletes. It does not
yet promise every optional CalDAV/CardDAV extension, scheduling, or every
client's proprietary discovery flow. An automated black-box protocol suite
checks the supported contract over a real loopback HTTP listener on every full
CI run. The separate compatibility matrix against named third-party clients
remains an explicit public-beta exit criterion in the roadmap.
