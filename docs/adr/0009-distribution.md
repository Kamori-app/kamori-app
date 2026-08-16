# ADR 0009: Distribution channels

Status: accepted

## Decision

Canonical IDs are `app.kamori.desktop` and `app.kamori.mobile`. Desktop ships
direct signed artifacts and a self-hosted signed Flatpak repository. Android
ships Play AAB, Accrescent APKS, and a universal signed APK with one stable
product certificate. iOS ships through TestFlight/App Store workflows.

## Consequences

Flathub and actual Accrescent publication are external-policy dependent and do
not block repository readiness. Distribution-specific builds use their own
update rules while sharing monotonically increasing product versions.
