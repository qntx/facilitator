# syntax=docker/dockerfile:1
# ============================================================================
# x402 Facilitator — Production Dockerfile
#
# Multi-stage build (requires Docker BuildKit):
#   1. chef    — install cargo-chef for dependency caching
#   2. planner — compute dependency recipe from Cargo.lock
#   3. builder — compile dependencies (cached) then the release binary
#   4. runtime — minimal Debian image with only the binary
#
# Build:
#   docker build -t x402-facilitator .
#   docker build -t x402-facilitator --build-arg FEATURES=chain-eip155 .
#
# Run:
#   docker run -p 8080:8080 -v ./config.toml:/app/config.toml x402-facilitator
# ============================================================================

ARG RUST_VERSION=1.93

# ------------------ Stage 1: Chef (dependency caching) ----------------------
FROM rust:${RUST_VERSION}-bookworm AS chef

RUN --mount=type=cache,target=/usr/local/cargo/registry \
    cargo install cargo-chef@0.1.71 --locked
WORKDIR /src

# ------------------ Stage 2: Prepare recipe ---------------------------------
FROM chef AS planner

COPY . .
RUN cargo chef prepare --recipe-path recipe.json

# ------------------ Stage 3: Build dependencies + binary --------------------
FROM chef AS builder

ARG FEATURES=default

COPY --from=planner /src/recipe.json recipe.json
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    cargo chef cook --release --features "${FEATURES}" --recipe-path recipe.json

COPY . .
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    cargo build --release --features "${FEATURES}" --bin facilitator \
    && cp target/release/facilitator /usr/local/bin/facilitator \
    && strip /usr/local/bin/facilitator

# ------------------ Stage 4: Minimal runtime image --------------------------
FROM debian:bookworm-slim AS runtime

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/* \
    && groupadd --system facilitator \
    && useradd --system --gid facilitator --create-home facilitator

WORKDIR /app

COPY --from=builder /usr/local/bin/facilitator /usr/local/bin/facilitator
RUN chown facilitator:facilitator /app

USER facilitator

ENV HOST=0.0.0.0
ENV PORT=8080

EXPOSE 8080

HEALTHCHECK --interval=15s --timeout=3s --start-period=10s --retries=3 \
    CMD curl -sf http://localhost:${PORT}/health || exit 1

ENTRYPOINT ["facilitator"]
CMD ["serve", "--config", "/app/config.toml"]
