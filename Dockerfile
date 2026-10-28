# syntax=docker/dockerfile:1

# Build the pinned helper on the digest-pinned official Rust image.
FROM rust:1.99.1-slim-bookworm@sha256:61e86bac21b4c1aeea7100008e1902d0048f9842fc94c7fb9c5f01baecbcb475 AS chef

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && dpkg --compare-versions "$(dpkg-query -W -f='${Version}' ca-certificates)" ge '20250419~deb12u2' \
    && rm -rf /var/lib/apt/lists/* \
    && cargo install cargo-chef --version 0.1.78 --locked

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
COPY assets ./assets

RUN cargo build --release --locked --features "${FEATURES}" --bin ironflow

FROM debian:bookworm-slim AS runtime

# Leave the private JSON store absent so the runtime UID creates and owns it.
# Root-group write access alone cannot chmod a root-owned store to 0700.
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl \
    && dpkg --compare-versions "$(dpkg-query -W -f='${Version}' ca-certificates)" ge '20250419~deb12u2' \
    && rm -rf /var/lib/apt/lists/* \
    && mkdir -p /data/flows \
    && chgrp -R 0 /data \
    && chmod -R g=u /data

COPY --from=builder /app/target/release/ironflow /usr/local/bin/ironflow
COPY assets/fonts/OFL.txt /usr/local/share/licenses/ironflow/fonts-OFL.txt

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
