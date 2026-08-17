# Kamori mobile

Kamori for Android and iOS is an offline-first encrypted client. It does not
run CalDAV or CardDAV on localhost. Calendar and contact integration is an
optional native projection described in
[`docs/whitepapers/mobile-system-integration.md`](../../docs/whitepapers/mobile-system-integration.md).

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
