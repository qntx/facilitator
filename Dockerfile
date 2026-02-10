# ============================================================================
# x402 LLM Gateway — Production Dockerfile
#
# Multi-stage build:
#   1. chef   — cache dependency layer via cargo-chef
#   2. build  — compile the release binary
#   3. runtime — minimal distroless image with only the binary
#
# Build:
#   docker build -t x402-gateway .
#
# Run:
#   docker run -p 3000:3000 -v ./gateway.toml:/app/gateway.toml x402-gateway
# ============================================================================

# Pin the Rust toolchain version for reproducible builds.
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

# Copy the dependency recipe and build dependencies first (cached layer).
COPY --from=planner /src/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json

# Copy full source and build the actual binary.
COPY . .
RUN cargo build --release --bin x402-llm-gateway \
    && strip target/release/x402-llm-gateway

# ------------------ Stage 4: Minimal runtime image --------------------------
FROM debian:bookworm-slim AS runtime

# Install only the minimal runtime dependencies (TLS).
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Create a non-root user for the gateway process.
RUN groupadd --gid 1000 gateway \
    && useradd --uid 1000 --gid gateway --shell /bin/false --create-home gateway

WORKDIR /app

# Copy the compiled binary from the builder stage.
COPY --from=builder /src/target/release/x402-llm-gateway /app/x402-gateway

# Own everything by the non-root user.
RUN chown -R gateway:gateway /app

USER gateway

# Default port exposed by the gateway.
EXPOSE 3000

# Health check — TCP probe via the built-in `health` subcommand.
HEALTHCHECK --interval=15s --timeout=5s --start-period=10s --retries=3 \
    CMD ["/app/x402-gateway", "health", "--port", "3000"]

# The config file is expected to be mounted at /app/gateway.toml.
# Override with: -v /path/to/your/gateway.toml:/app/gateway.toml
ENTRYPOINT ["/app/x402-gateway"]
CMD ["serve", "--config", "/app/gateway.toml"]
