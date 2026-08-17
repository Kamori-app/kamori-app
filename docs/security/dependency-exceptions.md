# Dependency audit exceptions

Last reviewed: 2026-08-17

Kamori treats every new RustSec advisory as a CI failure. The explicit
exceptions in `scripts/cargo-audit.sh` are target-specific warnings inherited
from the latest Tauri 2 Linux dependency graph; they are not silent global
waivers.

## Tauri Linux GTK3 graph

`RUSTSEC-2024-0411` through `RUSTSEC-2024-0420` mark the gtk-rs GTK3 bindings as
unmaintained. `RUSTSEC-2024-0370` is the unmaintained `proc-macro-error` used by
that graph. `RUSTSEC-2024-0429` reports an unsound iterator implementation in
`glib 0.18.5`. They arrive through Tauri 2.11.5, WRY, WebKitGTK, and the tray
implementation on Linux; Kamori does not depend on those crates directly.

The Linux artifact remains supported because Flatpak confines it and the
desktop client needs the platform WebView and tray. Builds use the current
Tauri release and the maintained OS WebKitGTK runtime. Remove these exceptions
as soon as Tauri moves its Linux runtime off the affected gtk-rs generation. If
an exploitable path is demonstrated before that migration, suspend Linux
artifacts instead of expanding the exception.

## URL pattern Unicode graph

`RUSTSEC-2025-0075`, `RUSTSEC-2025-0080`, `RUSTSEC-2025-0081`,
`RUSTSEC-2025-0098`, and `RUSTSEC-2025-0100` are unmaintained `unic-*` crates
used by `urlpattern 0.3.0` through `tauri-utils`. They contain no reported
vulnerability in the current audit. Remove the exceptions when Tauri replaces
that dependency or upgrades to a maintained Unicode implementation.

Dependabot checks the Cargo graph weekly. Every dependency update must run
`bash scripts/cargo-audit.sh`; adding or extending an exception requires a new
dated review in this file.
