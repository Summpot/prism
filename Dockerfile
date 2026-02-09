# syntax=docker/dockerfile:1

# NOTE: Prism's current implementation in this repository is Rust.
# The previous Go-based Dockerfile has been preserved as `Dockerfile.go`.

ARG RUST_MUSL_IMAGE=x86_64-musl-stable
ARG MUSL_TARGET=x86_64-unknown-linux-musl

# Build a static musl binary using https://github.com/BlackDex/rust-musl
FROM docker.io/blackdex/rust-musl:${RUST_MUSL_IMAGE} AS build

WORKDIR /home/rust/src

# Cache deps first.
COPY Cargo.toml Cargo.lock ./
COPY crates/prism/Cargo.toml crates/prism/Cargo.toml

# Cargo requires at least one target file (src/main.rs or src/lib.rs) to exist
# when parsing a package manifest. Create a tiny placeholder so `cargo fetch`
# can run before we copy the full source tree.
RUN mkdir -p crates/prism/src \
    && printf 'fn main() {}\n' > crates/prism/src/main.rs

RUN --mount=type=cache,target=/home/rust/.cargo/registry \
    --mount=type=cache,target=/home/rust/src/target \
    cargo fetch

# Copy the rest and build.
COPY . ./

ARG MUSL_TARGET
RUN --mount=type=cache,target=/home/rust/.cargo/registry \
    --mount=type=cache,target=/home/rust/src/target \
    cargo build --release -p prism --target ${MUSL_TARGET} \
    && cp -f /home/rust/src/target/${MUSL_TARGET}/release/prism /home/rust/prism

FROM alpine:3.20

ARG MUSL_TARGET

RUN apk add --no-cache ca-certificates \
    && addgroup -S -g 10001 prism \
    && adduser -S -D -H -u 10001 -G prism prism

# Default workdir on Linux is /var/lib/prism. Ensure the non-root user can write there.
RUN mkdir -p /var/lib/prism \
    && chown -R prism:prism /var/lib/prism

COPY --from=build /home/rust/prism /usr/local/bin/prism

# Prism auto-detects prism.toml > prism.yaml > prism.yml from CWD.
WORKDIR /config

EXPOSE 25565 8080 7000

USER prism

ENTRYPOINT ["/usr/local/bin/prism"]
