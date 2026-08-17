# Production-like acceptance tests

This directory contains Kamori's browser acceptance harness. It boots the
production container images against disposable PostgreSQL, Valkey, and
S3-compatible services, then drives the deployed web application with
Playwright. It is intended to catch integration and packaging failures that
unit tests cannot see.

## Prerequisites

- Bun at the version pinned in the root `package.json`.
- Podman with a running machine, or Docker with Compose v2.
- `openssl` and `curl` on the host.
- Playwright Chromium (`bunx playwright install chromium`).

The harness selects a working Podman installation first and falls back to
Docker. Override selection with `KAMORI_CONTAINER_ENGINE=podman` or
`KAMORI_CONTAINER_ENGINE=docker`.

## Commands

Run from the repository root:

```bash
# Build images, start the stack, and wait for every public endpoint.
bun run acceptance:up

# Fast release gate used for pull requests.
bun run acceptance:smoke

# Sharing, recovery, blob quota, and smoke scenarios.
bun run acceptance:full

# Inspect logs, stop containers, or also delete disposable data.
bun run acceptance:logs
bun run acceptance:down
bun run acceptance:reset
```

After `acceptance:up`, the endpoints are:

- web application: `http://127.0.0.1:14173/app`
- cloud API: `http://127.0.0.1:18080`
- operator console: `http://127.0.0.1:14174`

Set `KAMORI_ACCEPTANCE_SKIP_BUILD=true` when the three acceptance images have
already been loaded by CI or another build step.

## Coverage

The smoke scenario verifies readiness, web-only registration, OPAQUE sign-in,
device registration, encrypted collection creation, offline outbox replay,
task/event/contact synchronization, refresh-cookie session restoration, and
logout/sign-in recovery.

The full suite additionally verifies:

- editor and reader invite roles, single-use redemption, and reader write denial;
- recovery-kit password rotation and encrypted-data restoration;
- encrypted blob hash integrity, idempotent upload, exact download, invalid-hash
  rejection, and the strict storage quota.

Tests use unique accounts and one Playwright worker so quota and lifecycle
assertions remain deterministic. Failure artifacts are written under
`tests/acceptance/artifacts/` and are ignored by Git.

## Secrets and data lifecycle

The stack never reads repository or production secrets. On first start,
`scripts/acceptance.sh` generates random JWT, rotation, TOTP, metrics, and
OPAQUE setup secrets in `tests/acceptance/.runtime/` with owner-only
permissions. The directory is ignored by both Git and container build context.

`acceptance:down` removes containers and the network but preserves disposable
database/object-store volumes for faster reruns. `acceptance:reset` removes
those volumes and generated secrets. Never copy `.runtime` values into a real
deployment.

## CI policy

GitHub Actions runs `@smoke` for pull requests. Pushes to `main`, the nightly
schedule, and manual runs execute the full suite. CI builds the production
images first, loads local-API variants for the acceptance stack, and uploads
Playwright traces, screenshots, videos, and container logs only when a run
fails.

When a browser test fails locally, inspect the container logs and Playwright
trace before rerunning:

```bash
bun run acceptance:logs
bunx playwright show-trace tests/acceptance/artifacts/test-results/<test>/trace.zip
```
