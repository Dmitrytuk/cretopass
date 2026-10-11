# Isolated environment for testing `cretopass remove` / lifecycle commands.
#
# Provides a Rust 1.92.0 toolchain plus the tools `tests/test-remove.sh` needs
# (bash, mktemp, a C linker). The repo is mounted at runtime by
# scripts/docker/test-remove-lifecycle.sh, so no source is baked in here.
FROM rust:1.92.0-slim

RUN apt-get update && \
    apt-get install -y --no-install-recommends \
        ca-certificates \
        git && \
    rm -rf /var/lib/apt/lists/*

# Cargo registry cache lives here so it can be persisted via a named volume.
ENV CARGO_HOME=/cargo-cache
RUN mkdir -p /cargo-cache

WORKDIR /app