FROM lukemathwalker/cargo-chef:latest-rust-1 AS chef
WORKDIR /app

FROM chef AS planner
COPY . .
# Modify workspace to only include server and shared-proto for Docker build
RUN sed -i 's|"apps/desktop/src-tauri",||g' Cargo.toml && \
    sed -i 's|"libs/media",||g' Cargo.toml
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
COPY --from=planner /app/recipe.json recipe.json
# Build dependencies - this is the caching layer!
RUN cargo chef cook --release --recipe-path recipe.json
# Build application
COPY . .
# Modify workspace to only include server and shared-proto
RUN sed -i 's|"apps/desktop/src-tauri",||g' Cargo.toml && \
    sed -i 's|"libs/media",||g' Cargo.toml
RUN cargo build --release --package server

FROM debian:bookworm-slim AS runtime
WORKDIR /app
# Install runtime dependencies for SQLx (PostgreSQL client libs)
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    libpq5 \
    && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/server /usr/local/bin
ENTRYPOINT ["/usr/local/bin/server"]
