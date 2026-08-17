# Client release and signing

This runbook describes the tag-driven client artifact pipeline. It does not
publish to an app store and it always creates or reuses a **draft** GitHub
release. A human must complete the verification checklist before publishing
the draft or uploading an artifact to a store.

## Release inputs

1. Choose one semantic version and update all four version declarations:
   root `package.json`, desktop `Cargo.toml`, `tauri.conf.json`, and mobile
   `pubspec.yaml` (the Dart build suffix may differ).
2. Run `bun install --frozen-lockfile`, `bun run verify`, `bun run build`,
   `cargo fmt --all -- --check`, and
   `cargo clippy --workspace --all-targets -- -D warnings`.
3. Record results for the manual gates in `SPEC.md` against a disposable hosted
   stack.
4. Create a signed `vX.Y.Z` tag only from the reviewed release commit.

The workflow rejects a tag whose version differs from any client manifest.
Manual reruns accept only an existing tag; branch heads cannot be packaged as a
release.

## GitHub secrets

Desktop macOS signing and notarization:

- `APPLE_CERTIFICATE`: base64 Developer ID Application `.p12`;
- `APPLE_CERTIFICATE_PASSWORD`;
- `APPLE_SIGNING_IDENTITY`;
- `APPLE_ID`, `APPLE_PASSWORD` (an app-specific password), and `APPLE_TEAM_ID`.

Windows signing:

- `WINDOWS_CERTIFICATE`: base64 code-signing `.pfx`;
- `WINDOWS_CERTIFICATE_PASSWORD`;
- `WINDOWS_CERTIFICATE_THUMBPRINT`.

For newly issued certificates that cannot be exported to PFX, replace this
template with the issuer's HSM or trusted-signing command before releasing.
Do not weaken the workflow to ship an unsigned installer.

Linux and Flatpak signing:

- `FLATPAK_GPG_PRIVATE_KEY`: base64 export of the dedicated offline release
  subkey;
- `FLATPAK_GPG_KEY_ID`.

Android signing:

- `ANDROID_KEYSTORE_BASE64`;
- `ANDROID_KEYSTORE_PASSWORD`;
- `ANDROID_KEY_ALIAS`;
- `ANDROID_KEY_PASSWORD`.

The Android product key is a long-lived identity. Keep an encrypted offline
backup and test restoration before the first public release. Losing it can
make updates impossible outside store-managed key rotation.

## Produced artifacts

- macOS `.app`/DMG: Developer ID signed and notarized by Tauri;
- Windows installers: Authenticode signed and timestamped;
- Linux AppImage/deb/rpm: detached armored signatures;
- Flatpak bundle plus self-hosted signed OSTree repository on GNOME runtime 50;
- Android Play AAB, universal signed APK, and signed APKS for Accrescent;
- iOS unsigned `Runner.app` archive used only to verify the real Rust library is
  linked. It must never be presented as an end-user download.

The iOS store release remains a separate manual signing/TestFlight operation
until the Apple account, distribution certificate, provisioning profile, and
App Store Connect ownership exist. Add those credentials to a protected GitHub
environment and replace the unsigned archive job with `flutter build ipa`
before declaring iOS generally available.

## Approval checklist

Before publishing the draft release:

1. Verify the tag signature and compare every artifact digest with the workflow
   log.
2. Verify macOS notarization/stapling and Windows Authenticode timestamping on
   clean machines.
3. Install the Flatpak from the generated repository, verify its GPG trust
   path, and exercise login, sync, and DAV bridge start/stop.
4. Install the universal APK and an APK selected from the APKS archive on clean
   supported Android devices. Confirm the package contains the native Rust
   library and that password login, offline edits, sync, and opt-in projection
   work.
5. Confirm all clients point to the intended production API origin and that no
   development registration or mock-Rust switches are active.
6. Keep the GitHub release in draft if any store, signing, recovery, backup, or
   compatibility gate is missing.

## Channel notes

The signed Flatpak repository is the supported beta Linux channel. Flathub is
not a release dependency. Google Play and Accrescent receive the same Android
application ID and signing identity. Apple TestFlight/App Store publication is
explicitly outside the unsigned artifact job.
