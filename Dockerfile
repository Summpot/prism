# syntax=docker/dockerfile:1

# NOTE: Prism's current implementation in this repository is Rust.
# The previous Go-based Dockerfile has been preserved as `Dockerfile.go`.

FROM rust:1.85-slim-bookworm AS build

WORKDIR /src

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates pkg-config clang \
    && rm -rf /var/lib/apt/lists/*

# Cache deps first.
COPY Cargo.toml Cargo.lock ./
COPY crates/prism/Cargo.toml crates/prism/Cargo.toml

RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    cargo fetch

# Copy the rest and build.
COPY . ./

RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    cargo build --release -p prism

FROM debian:bookworm-slim

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && useradd -r -u 10001 -g nogroup prism

COPY --from=build /src/target/release/prism /usr/local/bin/prism

# Prism auto-detects prism.toml > prism.yaml > prism.yml from CWD.
WORKDIR /config

EXPOSE 25565 8080 7000

USER prism

ENTRYPOINT ["/usr/local/bin/prism"]
