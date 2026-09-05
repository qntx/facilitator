# syntax=docker/dockerfile:1
#
# Multi-stage build. Runtime is distroless (no shell, no curl).
# HEALTHCHECK is omitted: compose/k8s HTTP probes on /healthz and /readyz.
#
#   docker build -t ghcr.io/qntx/facilitator:0.7.0 .

ARG RUST_VERSION=1.95

FROM rust:${RUST_VERSION}-bookworm AS builder

WORKDIR /src/facilitator
COPY . .

RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/src/facilitator/target \
    cargo build --release --locked --bin facilitator \
    && cp /src/facilitator/target/release/facilitator /usr/local/bin/facilitator

FROM gcr.io/distroless/cc-debian12:nonroot

COPY --from=builder --chown=65532:65532 /usr/local/bin/facilitator /usr/bin/facilitator
# Do not COPY --chmod=644 into a new directory: BuildKit applies 0644 to the
# directory as well (no +x), so USER 65532 cannot traverse /etc/facilitator.
COPY --chown=65532:65532 config.example.toml /etc/facilitator/config.toml

# In-container bind. Host example config stays 127.0.0.1; compose/k8s overlay the same vars.
ENV FACILITATOR_HTTP_LISTEN=0.0.0.0:8080
ENV FACILITATOR_HTTP_METRICS_LISTEN=0.0.0.0:9090

EXPOSE 8080 9090

ENTRYPOINT ["/usr/bin/facilitator"]
CMD ["serve", "-c", "/etc/facilitator/config.toml"]
