FROM rust:1-bookworm AS builder

WORKDIR /app

ARG FEATURES="postgres redis"

COPY Cargo.toml Cargo.lock ./
COPY src ./src

RUN cargo build --release --features "${FEATURES}" --bin ironflow

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
