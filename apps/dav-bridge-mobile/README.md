# Kamori mobile

Kamori for Android and iOS is an offline-first encrypted client. It does not
run CalDAV or CardDAV on localhost. Calendar and contact integration is an
optional native projection described in
[`docs/whitepapers/mobile-system-integration.md`](../../docs/whitepapers/mobile-system-integration.md).
Calendar and Contacts projection is enabled explicitly for each space; enabling
one space never exposes another space to the operating-system data stores.

The mobile client signs into an existing account, provisions an independent
device identity, hydrates current space keys from recovery packages, stores its
SQLCipher runtime snapshot for background work, and edits calendars, tasks,
and contacts through the signed encrypted operation log. Owner-created invite
codes first perform a full sync and atomic current-state key rotation; the
rotated key and recovery cursor are persisted before the code is shown.
Device identities and account master keys are stored in platform secure storage
under a scope derived from both the normalized server origin and username.
Signing into another account can therefore never overwrite an existing
account's device identity, including when the new device enrollment fails.
The current refresh token and a random per-generation retry id are stored
together in Android Keystore/iOS Keychain-backed storage before authenticated
background work. SQLCipher persists the same retry state for the shared sync
runner, so a process restart or lost response cannot turn a legitimate retry
into token-reuse revocation.

Native mobile passkeys are intentionally post-MVP. Current mobile sign-in is
OPAQUE password plus optional TOTP; registration remains web-only.

## Development

```bash
flutter pub get
flutter analyze
flutter test
```

Debug builds use the mock Rust API unless explicitly disabled:

```bash
flutter run --dart-define=KAMORI_USE_MOCK_RUST=false
```

Production and integration builds must package the real Rust library.

## FRB bindings

The Rust surface lives in `packages/crypto-core-lib/src/frb_api.rs`. Generated
Rust and Dart bindings are committed. Regenerate from the repository root:

```bash
flutter_rust_bridge_codegen generate \
  --rust-input crate::frb_api \
  --rust-root packages/crypto-core-lib \
  --rust-output packages/crypto-core-lib/src/frb_generated.rs \
  --dart-output apps/dav-bridge-mobile/lib/src/rust/gen \
  --rust-features frb \
  --stop-on-error
```

The codegen binary and Dart package must use the same FRB version.

## Release artifacts

Android uses `app.kamori.mobile` and compile SDK 37. Release signing is read
only from the `KAMORI_ANDROID_*` environment variables; a missing keystore
never falls back to the debug identity. The tag workflow emits a Play AAB, a
universal APK, and an Accrescent APKS archive.

iOS release builds require Xcode plus Apple signing credentials. CI first
builds an unsigned archive as a native-linkage gate; it is not an installable
public release. See
[`docs/runbooks/client-release.md`](../../docs/runbooks/client-release.md).
