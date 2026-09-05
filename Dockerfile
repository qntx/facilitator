# syntax=docker/dockerfile:1
#
# Multi-stage build. Runtime is distroless (no shell, no curl).
# HEALTHCHECK is omitted: compose/k8s HTTP probes on /healthz and /readyz.
#
#   docker buildx build --load --platform linux/amd64 -t ghcr.io/qntx/facilitator:0.7.1 .
#   OrbStack: docker-buildx build --load --platform linux/amd64 …

ARG RUST_VERSION=1.95

FROM rust:${RUST_VERSION}-bookworm AS builder

WORKDIR /src/facilitator
COPY . .

RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/src/facilitator/target \
    cargo build --release --locked --bin facilitator \
    && cp /src/facilitator/target/release/facilitator /usr/local/bin/facilitator \
    && mkdir -p /out/etc/facilitator \
    && cp /src/facilitator/config.example.toml /out/etc/facilitator/config.toml \
    && chmod 0755 /out/etc/facilitator \
    && chmod 0644 /out/etc/facilitator/config.toml

FROM gcr.io/distroless/cc-debian12:nonroot

COPY --from=builder --chown=65532:65532 /usr/local/bin/facilitator /usr/bin/facilitator
# Do not COPY --chmod. BuildKit applies --chmod to a newly created dest
# directory (0.7.0: /etc/facilitator became 0644, no +x; moby/moby#49851).
# COPY the parent: /etc already exists in distroless, so facilitator/ is
# source content (0755 in the builder) rather than a created dest dir.
COPY --from=builder --chown=65532:65532 /out/etc /etc

# In-container bind. Host example config stays 127.0.0.1; compose/k8s overlay the same vars.
# Non-loopback listen requires [http.auth] + FACILITATOR_API_TOKEN (baked example has the table).
ENV FACILITATOR_HTTP_LISTEN=0.0.0.0:8080
ENV FACILITATOR_HTTP_METRICS_LISTEN=0.0.0.0:9090

EXPOSE 8080 9090

ENTRYPOINT ["/usr/bin/facilitator"]
CMD ["serve", "-c", "/etc/facilitator/config.toml"]
