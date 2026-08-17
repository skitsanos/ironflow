# syntax=docker/dockerfile:1

FROM lukemathwalker/cargo-chef:0.1.78-rust-1.97.1-slim-bookworm@sha256:e406ad0baa7266cee09ca9f62f30d7ed330bdb25be9f337ff8090e7ae215f7fd AS chef

WORKDIR /app

ARG FEATURES="postgres redis"

FROM chef AS planner

COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY tools ./tools

RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder

ARG FEATURES="postgres redis"

COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release --locked --features "${FEATURES}" \
    --bin ironflow --recipe-path recipe.json

COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY tools ./tools

RUN cargo build --release --locked --features "${FEATURES}" --bin ironflow

FROM debian:bookworm-slim AS runtime

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/* \
    && mkdir -p /data/runs /data/flows \
    && chgrp -R 0 /data \
    && chmod -R g=u /data

COPY --from=builder /app/target/release/ironflow /usr/local/bin/ironflow

ENV HOST=0.0.0.0 \
    PORT=3000 \
    IRONFLOW_STORE_DIR=/data/runs \
    FLOWS_DIR=/data/flows \
    HOME=/tmp

WORKDIR /data
# OpenShift may replace this UID from the namespace's allocated range. Writable
# paths are root-group writable so both that arbitrary UID and local UID 1001
# can run without an anyuid SCC.
USER 1001:0

EXPOSE 3000

HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
    CMD curl --fail "http://127.0.0.1:${PORT}/health/live" || exit 1

ENTRYPOINT ["/usr/local/bin/ironflow"]
CMD ["serve"]
