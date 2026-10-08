# The dedicated multiplayer server (D24, M4-8). Single player doesn't use it: the
# client launches pax_server itself (NETWORK_PROTOCOL §6).
#
#   docker compose up                       # see compose.yaml
#   docker build -t pax-server . && docker run -p 7777:7777 -v pax-saves:/app/saves pax-server
#
# Settings come from PAX_* environment variables: docker/entrypoint.sh is the list,
# and docs/HOSTING.md explains them. Arguments after the image name go straight to
# pax_server.

# The toolchain rust-toolchain.toml pins: the base image's, so rustup has nothing
# to fetch (the builder copies rust-toolchain.toml in either way; CI checks they match).
ARG RUST_VERSION=1.99

# Dependencies are built in their own layer (cargo-chef), so a code change doesn't
# rebuild them.
FROM rust:${RUST_VERSION}-bookworm AS chef
RUN cargo install cargo-chef --locked --version 0.1.71
WORKDIR /src

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
# The pinned toolchain before the dependency layer, so both builds use it (a newer
# pin then rebuilds the layer once, not on every code change).
COPY rust-toolchain.toml rust-toolchain.toml
COPY --from=planner /src/recipe.json recipe.json
RUN cargo chef cook --release -p pax_server --recipe-path recipe.json
COPY . .
# The protocol's generated code is checked in (pax_protocol, M3-1): no flatc here.
RUN cargo build --release -p pax_server

FROM debian:bookworm-slim
# openssl makes the server's certificate once (docker/entrypoint.sh).
RUN apt-get update \
    && apt-get install -y --no-install-recommends openssl \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --uid 10001 --home-dir /app --shell /usr/sbin/nologin pax \
    && mkdir -p /app/saves /app/tls \
    && chown pax /app/saves /app/tls
WORKDIR /app
COPY --from=builder /src/target/release/pax_server /usr/local/bin/pax_server
COPY data /app/data
COPY scenarios /app/scenarios
COPY docker/entrypoint.sh /usr/local/bin/pax-entrypoint
USER pax
# Saves, and the TLS certificate whose fingerprint players pin: keep both across
# restarts and upgrades.
VOLUME ["/app/saves", "/app/tls"]
ENV PAX_PORT=7777
EXPOSE 7777
# A TCP probe on the port (bash's /dev/tcp; the server needs no health endpoint).
HEALTHCHECK --interval=30s --timeout=3s --start-period=20s --retries=3 \
    CMD ["bash", "-c", "exec 3<>/dev/tcp/127.0.0.1/${PAX_PORT}"]
ENTRYPOINT ["pax-entrypoint"]
