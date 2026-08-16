# syntax=docker/dockerfile:1.7
FROM rust:1.97.1-slim-bookworm AS builder

WORKDIR /workspace
# Package revisions follow the pinned Debian base image so rebuilding the image
# receives its current security updates instead of stale revision pins.
# hadolint ignore=DL3008
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates pkg-config \
    && rm -rf /var/lib/apt/lists/*

COPY . .
RUN cargo build --locked -p cloud-server --release

FROM debian:bookworm-slim AS runtime
# hadolint ignore=DL3008
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --uid 10001 --create-home kamori

WORKDIR /app
COPY --from=builder /workspace/target/release/cloud-server /usr/local/bin/cloud-server
COPY --from=builder /workspace/apps/cloud-server/migrations /app/migrations

ENV RUST_LOG=info
EXPOSE 8080
USER 10001:10001

ENTRYPOINT ["/usr/local/bin/cloud-server"]
