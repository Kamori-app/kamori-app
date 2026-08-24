# Kamori

Kamori is an end-to-end encrypted operation platform. The first product release
is an offline-capable calendar, task, and contact suite for individuals with
multiple devices and small trusted groups.

## Project status

The repository contains the hosted-beta architecture and clients, but no public
production release has been declared. Automated gates cover the core protocol,
server, web, desktop, mobile, containers, and infrastructure; the manual
release exercises listed in [SPEC.md](SPEC.md) still require recorded results
with production signing and operator credentials.

Do not treat an unsigned development build as a supported backup service.

## Product boundaries

- Web and mobile are first-party PIM clients.
- Desktop is a control center and local CalDAV/CardDAV bridge.
- Android/iOS system calendar and contact projection is explicit opt-in.
- Mobile does not run a localhost DAV server.
- The cloud stores signed encrypted operations and snapshots; DAV is not the
  server data model.
- The server cannot decrypt user PIM content.

## Repository layout

- `apps/cloud-server` — Rust/Axum API and background workers.
- `apps/web-frontend` — SvelteKit landing and offline web client.
- `apps/dav-bridge-desktop` — Tauri desktop control center and DAV bridge.
- `apps/dav-bridge-mobile` — Flutter Android/iOS PIM client.
- `packages/crypto-core-lib` — shared Rust crypto, protocol, sync, PIM, and DAV core.
- `docs` — architecture decisions, roadmap, protocols, and guides.
- `SPEC.md` — normative product contract.

## Tooling

Kamori uses Bun workspaces and Turborepo plus a Rust Cargo workspace. Mobile is
Flutter with committed Flutter Rust Bridge bindings.

Current repository pins are recorded in `package.json`, `rust-toolchain.toml`,
Flutter project files, and lockfiles.

## Development

Install JavaScript dependencies:

```bash
bun install
```

Fast Rust feedback loop:

```bash
bun run fast
```

Strict repository verification:

```bash
bun run verify
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Production-like browser acceptance tests require a running Podman or Docker
engine:

```bash
bun run acceptance:smoke
bun run acceptance:full
```

The stack, coverage, generated-secret lifecycle, and troubleshooting workflow
are documented in [tests/acceptance/README.md](tests/acceptance/README.md).

Pull-request and `main` CI classifies changed paths before starting expensive
jobs. Native Android/iOS packages, Rust, hosted containers, frontends, and
Pulumi checks run only when their inputs or shared dependencies changed. The
mapping and its regression tests live in `scripts/ci/`; `CI required` is the
single stable branch-protection result. A full matrix still runs on manual
dispatch, CI workflow changes, and the weekly compatibility schedule.

Targeted applications:

```bash
bun run --filter cloud-server dev
bun run --filter web-frontend dev
bun run --filter dav-bridge-desktop tauri:dev

cd apps/dav-bridge-mobile
flutter pub get
flutter run
```

Server setup and required environment variables are documented in
`apps/cloud-server/README.md` and the hosted-beta runbook. Never commit secrets
or use production credentials in local examples.

The exact production bootstrap procedure for encrypted Pulumi configuration
and GitHub Actions is documented in [SECRETS.md](SECRETS.md).

## Documentation

- [Product contract](SPEC.md)
- [Documentation index](docs/README.md)
- [Architecture overview](docs/architecture/overview.md)
- [Architecture decisions](docs/adr/README.md)
- [Roadmap](docs/ROADMAP.md)

Historical v1/v2/v3 design documents were removed. They are not supported or
normative.

## Licensing

Kamori uses a component-specific AGPL/Apache/Creative Commons split. See
[LICENSE.md](LICENSE.md). Canonical license texts and the trademark policy are
included. The contributor agreement remains an organization-neutral draft;
the complete legal package must be reviewed before public source publication.

## Contributing and security

The project is not yet accepting substantial external contributions. A reviewed
CLA, contribution guide, and responsible disclosure process are release
prerequisites. Do not submit security vulnerabilities through public issues.
Known target-specific dependency audit exceptions and their removal conditions
are documented in [docs/security/dependency-exceptions.md](docs/security/dependency-exceptions.md).
