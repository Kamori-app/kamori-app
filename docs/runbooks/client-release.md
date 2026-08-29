# Client release and signing

This runbook describes the tag-driven client artifact pipeline. It does not
publish to an app store and it always creates or reuses a **draft** GitHub
release. A human must complete the verification checklist before publishing
the draft or uploading an artifact to a store.

iOS packaging is disabled by default while the public site labels iOS as
Coming Soon. A tag builds native desktop/Flatpak and Android artifacts. The
iOS job runs only when the repository variable
`KAMORI_RELEASE_IOS_ENABLED=true`, or when an operator manually packages an
existing tag with the `include_ios` input enabled. Enabling the job creates a
signed IPA but does not upload it to TestFlight or the App Store.

## Release inputs

1. Choose one semantic version and update all four version declarations:
   root `package.json`, desktop `Cargo.toml`, `tauri.conf.json`, and mobile
   `pubspec.yaml` (the Dart build suffix may differ).
2. Run `bun install --frozen-lockfile`, `bun run verify`, `bun run build`,
   `cargo fmt --all -- --check`, and
   `cargo clippy --workspace --all-targets -- -D warnings`.
3. Copy [the release evidence template](release-evidence-template.md) to the
   private operator evidence store and record the manual gates from `SPEC.md`
   against a disposable hosted stack. Never commit secrets, private hostnames,
   user data, or unredacted security evidence.
4. Create a signed `vX.Y.Z` tag only from the reviewed release commit.

The workflow rejects a tag whose version differs from any client manifest.
Manual reruns accept only an existing tag; branch heads cannot be packaged as a
release.

## Protected GitHub environment

Create a GitHub Actions environment named `release`, require an operator
approval, and store every signing secret below in that environment. Do not use
repository-wide secrets for signing identities: environment approval keeps a
tag push from reading them until a human confirms the intended release.

The environment can contain credentials for a channel before that channel is
enabled. The iOS job remains skipped unless one of the explicit opt-ins above
is active.

`KAMORI_RELEASE_IOS_ENABLED` is a repository Actions variable, not a secret.
Leave it absent or set it to `false` while iOS is Coming Soon. Set it to `true`
only after the TestFlight channel and all iOS signing credentials are ready.
This variable controls tag-push releases; the `include_ios` input controls
manual runs independently.

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

iOS distribution signing (required only when the iOS job is enabled):

- `IOS_CERTIFICATE`: base64 Apple Distribution `.p12`;
- `IOS_CERTIFICATE_PASSWORD`;
- `IOS_PROVISIONING_PROFILE`: base64 App Store distribution
  `.mobileprovision` for `app.kamori.mobile`;
- `IOS_KEYCHAIN_PASSWORD`: a random, workflow-only temporary keychain
  password;
- shared `APPLE_TEAM_ID` from the macOS section.

Keep the Apple distribution certificate and profile in an encrypted offline
backup. Profiles expire and must be rotated before a release; the workflow
must fail instead of falling back to unsigned packaging.

## Produced artifacts

- macOS `.app`/DMG: Developer ID signed and notarized by Tauri;
- Windows installers: Authenticode signed and timestamped;
- Linux AppImage/deb/rpm: detached armored signatures;
- Flatpak bundle plus self-hosted signed OSTree repository on GNOME runtime 50;
- Android Play AAB, universal signed APK, and signed APKS for Accrescent;
- when explicitly enabled, an Apple-distribution-signed iOS IPA with the real
  Rust library linked.

TestFlight and App Store upload remain separate operator actions. Do not expose
the IPA as an end-user download or declare iOS available until App Store
Connect ownership, TestFlight review, and the physical-device checklist are
complete.

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
6. If iOS packaging is enabled, validate installation and the sign-in,
   offline, sync, recovery, and opt-in system-projection paths through
   TestFlight on physical supported devices.
7. Keep the GitHub release in draft if any store, signing, recovery, backup, or
   compatibility gate is missing.

## Channel notes

The signed Flatpak repository is the supported beta Linux channel. Flathub is
not a release dependency. Google Play and Accrescent receive the same Android
application ID and signing identity. Apple TestFlight/App Store publication is
explicitly outside the artifact workflow.
