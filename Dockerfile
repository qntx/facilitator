# syntax=docker/dockerfile:1
#
# Multi-stage build. Runtime is distroless (no shell, no curl).
# HEALTHCHECK is omitted: compose/k8s HTTP probes on /healthz and /readyz.
#
# Path deps: crates/facilitator → ../../../r402. The builder clones
# qntx/r402 at R402_REF (default v0.19.1) next to this repo.
#
#   docker build -t ghcr.io/qntx/facilitator:2.0.0 .

ARG RUST_VERSION=1.95

FROM rust:${RUST_VERSION}-bookworm AS builder

ARG R402_REF=v0.19.1
ENV GIT_TERMINAL_PROMPT=0
RUN git clone --depth 1 --branch "${R402_REF}" https://github.com/qntx/r402.git /src/r402 \
    && test -f /src/r402/crates/r402-protocol/Cargo.toml

WORKDIR /src/facilitator
COPY . .

RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/src/facilitator/target \
    cargo build --release --locked --bin facilitator \
    && cp /src/facilitator/target/release/facilitator /usr/local/bin/facilitator

FROM gcr.io/distroless/cc-debian12:nonroot

COPY --from=builder /usr/local/bin/facilitator /usr/bin/facilitator
COPY --chmod=644 config.example.toml /etc/facilitator/config.toml

# In-container bind. Host example config stays 127.0.0.1; compose/k8s overlay the same vars.
ENV FACILITATOR_HTTP_LISTEN=0.0.0.0:8080
ENV FACILITATOR_HTTP_METRICS_LISTEN=0.0.0.0:9090

EXPOSE 8080 9090

ENTRYPOINT ["/usr/bin/facilitator"]
CMD ["serve", "-c", "/etc/facilitator/config.toml"]
