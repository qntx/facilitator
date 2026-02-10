# ============================================================================
# x402 Facilitator — Production Dockerfile
#
# Multi-stage build:
#   1. chef    — cache dependency layer via cargo-chef
#   2. planner — prepare the dependency recipe
#   3. builder — compile the release binary
#   4. runtime — minimal image with only the binary
#
# Build:
#   docker build -t x402-facilitator .
#
# Run:
#   docker run -p 4021:4021 -v ./config.json:/app/config.json x402-facilitator
# ============================================================================

ARG RUST_VERSION=1.93

# ------------------ Stage 1: Chef (dependency caching) ----------------------
FROM rust:${RUST_VERSION}-bookworm AS chef

RUN cargo install cargo-chef --locked
WORKDIR /src

# ------------------ Stage 2: Prepare recipe ---------------------------------
FROM chef AS planner

COPY . .
RUN cargo chef prepare --recipe-path recipe.json

# ------------------ Stage 3: Build dependencies + binary --------------------
FROM chef AS builder

COPY --from=planner /src/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json

COPY . .
RUN cargo build --release --bin facilitator \
    && strip target/release/facilitator

# ------------------ Stage 4: Minimal runtime image --------------------------
FROM debian:bookworm-slim AS runtime

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/* \
    && groupadd --system facilitator \
    && useradd --system --gid facilitator --create-home facilitator

WORKDIR /app

COPY --from=builder /src/target/release/facilitator /usr/local/bin/facilitator

RUN chown facilitator:facilitator /app

USER facilitator

EXPOSE 4021

HEALTHCHECK --interval=15s --timeout=3s --start-period=5s --retries=3 \
    CMD curl -sf http://localhost:4021/health || exit 1

ENTRYPOINT ["facilitator"]
