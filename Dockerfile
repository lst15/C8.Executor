FROM rust:1-bookworm AS builder
WORKDIR /app

COPY Cargo.toml Cargo.lock ./
COPY src ./src

RUN cargo build --release --bin c8-executor

FROM debian:bookworm-slim AS runtime
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates libssl3 libsasl2-2 libzstd1 liblz4-1 \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY --from=builder /app/target/release/c8-executor /usr/local/bin/c8-executor

EXPOSE 8087
CMD ["c8-executor"]
