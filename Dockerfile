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
    && useradd --system --create-home --home-dir /home/ironflow --shell /usr/sbin/nologin ironflow \
    && mkdir -p /data/runs /data/flows \
    && chown -R ironflow:ironflow /data /home/ironflow

COPY --from=builder /app/target/release/ironflow /usr/local/bin/ironflow

ENV HOST=0.0.0.0 \
    PORT=3000 \
    IRONFLOW_STORE_DIR=/data/runs \
    FLOWS_DIR=/data/flows

WORKDIR /data
USER ironflow

EXPOSE 3000

HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
    CMD curl --fail "http://127.0.0.1:${PORT}/health" || exit 1

ENTRYPOINT ["/usr/local/bin/ironflow"]
CMD ["serve"]
