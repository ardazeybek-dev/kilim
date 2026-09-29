# Reproducible toolchain: build and test without installing Rust locally.
FROM rust:1-slim-bookworm
RUN apt-get update && apt-get install -y --no-install-recommends curl ca-certificates pkg-config libssl-dev \
    && rm -rf /var/lib/apt/lists/* \
    && rustup target add wasm32-unknown-unknown \
    && rustup component add clippy rustfmt \
    && cargo install wasm-pack --locked
WORKDIR /work
