# syntax=docker/dockerfile:1.7
FROM rust:1.98.0-slim-bookworm AS builder

WORKDIR /workspace
ENV RUSTUP_TOOLCHAIN=1.97.1
# Package revisions follow the pinned Debian base image so rebuilding the image
# receives its current security updates instead of stale revision pins.
# hadolint ignore=DL3008
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates libssl-dev pkg-config \
    && rm -rf /var/lib/apt/lists/*

# Compile the dependency graph in a manifest-only layer. Backend source changes
# then rebuild only Kamori crates, while Cargo.lock or manifest changes still
# invalidate the dependency cache. Keep every workspace manifest present so
# Cargo resolves the exact same locked graph as the final build.
COPY Cargo.toml Cargo.lock ./
COPY packages/crypto-core-lib/Cargo.toml packages/crypto-core-lib/Cargo.toml
COPY apps/cloud-server/Cargo.toml apps/cloud-server/Cargo.toml
COPY apps/dav-bridge-desktop/src-tauri/Cargo.toml apps/dav-bridge-desktop/src-tauri/Cargo.toml
RUN install -d packages/crypto-core-lib/src apps/cloud-server/src \
        apps/dav-bridge-desktop/src-tauri/src \
    && printf 'pub fn dependency_cache_placeholder() {}\n' \
        > packages/crypto-core-lib/src/lib.rs \
    && printf 'fn main() {}\n' > apps/cloud-server/src/main.rs \
    && printf 'pub fn dependency_cache_placeholder() {}\n' \
        > apps/dav-bridge-desktop/src-tauri/src/lib.rs \
    && cargo build --locked -p cloud-server --release

COPY packages/crypto-core-lib/src packages/crypto-core-lib/src
COPY apps/cloud-server/src apps/cloud-server/src
COPY apps/cloud-server/migrations apps/cloud-server/migrations
# COPY preserves host mtimes, which can be older than the placeholder build.
# Touch only first-party inputs so Cargo recompiles Kamori crates while reusing
# the dependency artifacts from the previous layer.
RUN find packages/crypto-core-lib/src apps/cloud-server/src \
        apps/cloud-server/migrations -type f -exec touch {} + \
    && cargo build --locked -p cloud-server --release

FROM debian:bookworm-slim AS runtime
# hadolint ignore=DL3008
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates libssl3 \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --uid 10001 --create-home kamori

WORKDIR /app
COPY --from=builder /workspace/target/release/cloud-server /usr/local/bin/cloud-server
COPY --from=builder /workspace/apps/cloud-server/migrations /app/migrations

ENV RUST_LOG=info
EXPOSE 8080
USER 10001:10001

ENTRYPOINT ["/usr/local/bin/cloud-server"]
