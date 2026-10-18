#!/usr/bin/env bash
# Dedicated synthetic PostgreSQL only; each workload owns its random schema.
set -euo pipefail
: "${KAMORI_DATABASE_URL:?Set KAMORI_DATABASE_URL to a dedicated test PostgreSQL database}"
repository=$(cd "$(dirname "$0")/../.." && pwd)
cd "$repository"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$repository/target}"
artifacts="${KAMORI_CONTENTION_ARTIFACTS:-${TMPDIR:-/tmp}/kamori-contention}"
mkdir -p "$artifacts"
{
  git rev-parse HEAD
  git status --short
  uname -a
  rustup run 1.99.0 rustc --version
  rustup run 1.99.0 cargo --version
} > "$artifacts/environment.log"
# Run serially for comparable timing; --nocapture persists exact counters and
# blocker PIDs. Do not run unrelated builds during the measurement window.
rustup run 1.99.0 cargo test -p cloud-server readiness_ -- --nocapture --test-threads=1 2>&1 | tee "$artifacts/server-measurements.log"
rustup run 1.99.0 cargo test -p crypto-core-lib --features frb readiness_slow_http -- --nocapture --test-threads=1 2>&1 | tee "$artifacts/native-measurements.log"
