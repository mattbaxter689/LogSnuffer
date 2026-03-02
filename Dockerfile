# 1. Use the bookworm variant to match the runtime's glibc (2.36)
FROM rust:1.93-bookworm AS builder

# Install build dependencies
RUN apt-get update && \
    apt-get install -y pkg-config libssl-dev && \
    rm -rf /var/lib/apt/lists/*

WORKDIR /usr/src/logsnuffer

# 2. Leverage BuildKit cache mounts for the cargo registry and target folder
# This replaces the "dummy src" method and is much faster.
RUN --mount=type=bind,source=Cargo.toml,target=Cargo.toml \
    --mount=type=bind,source=Cargo.lock,target=Cargo.lock \
    --mount=type=bind,source=src,target=src \
    --mount=type=cache,target=/usr/src/logsnuffer/target \
    --mount=type=cache,target=/usr/local/cargo/registry \
    cargo build --release --bin server --bin generator && \
    cp target/release/server /app-server && \
    cp target/release/generator /app-generator

# --- Runtime Image ---
FROM debian:bookworm-slim

# Install runtime-only dependencies
RUN apt-get update && \
    apt-get install -y libsqlite3-0 libssl3 ca-certificates && \
    rm -rf /var/lib/apt/lists/*

WORKDIR /app

# 3. Copy binaries directly from the builder's temporary /app- path
COPY --from=builder /app-server /app/server
COPY --from=builder /app-generator /app/generator

RUN chmod +x /app/server /app/generator

# Default command
CMD ["/app/server"]
