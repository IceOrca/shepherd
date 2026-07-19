FROM rust:1-slim-bookworm AS builder
WORKDIR /app/shepherd_backend

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
    pkg-config \
    ca-certificates \
    libssl-dev \
    ripgrep \
    && rm -rf /var/lib/apt/lists/*

RUN rustup component add rustfmt
RUN rustup component add clippy
RUN cargo install sqlx-cli --no-default-features --features postgres,rustls
