# Facilitator

[![CI][ci-badge]][ci-url]
[![License][license-badge]][license-url]
[![Rust][rust-badge]][rust-url]

[ci-badge]: https://github.com/qntx/facilitator/actions/workflows/ci.yml/badge.svg
[ci-url]: https://github.com/qntx/facilitator/actions/workflows/ci.yml
[license-badge]: https://img.shields.io/badge/license-MIT%2FApache--2.0-blue.svg
[license-url]: #license
[rust-badge]: https://img.shields.io/badge/rust-1.95%20%2B%20edition%202024-orange.svg
[rust-url]: https://doc.rust-lang.org/edition-guide/

**[x402](https://www.x402.org/) V2 facilitator process** — verifies payment payloads and settles transactions on-chain over HTTP.

Built on [r402](https://github.com/qntx/r402) **0.19.1**. This is facilitator 2.0: there is no compatibility with 1.0.0 config, r402 0.17, `/health`, Watchtower, or `fctl`. Replace `config.toml`; do not convert.

This process constructs in-process EVM `exact`/`upto`/`auth-capture`/`batch-settlement` and SVM `exact`/`upto` facilitators (one provider `Arc` per network, process `SettlementCache`). SVM upto uses process-wide `InMemoryChannelStorage`; Path 2 is a startup error. r402 0.19.1 has no rent-cleanup manager. Serves spec §7 HTTP plus `/healthz`, `/readyz`, and an optional metrics listen. A listed scheme without a constructor, or an empty constructed map, is a startup error.

v1 is a **single replica**. In-memory stores are process-local. Enabling `batch-settlement` logs `batch-settlement requires a single replica` at startup.

`crates/facilitator` path-depends on a **sibling** r402 checkout (`../../../r402/crates/...` from that crate). Clone both repos next to each other; CI clones `qntx/r402` at tag `v0.19.1` into the same layout.

```text
<parent>/
  facilitator/    # this repo
  r402/           # git clone --branch v0.19.1 https://github.com/qntx/r402
```

## Quick Start

```bash
# from <parent>/facilitator, with <parent>/r402 present
cargo run -p facilitator -- init
# edit config.toml — named [signer.*] tables, env/file secrets only
cargo run -p facilitator -- validate -c config.toml
cargo run -p facilitator -- serve -c config.toml
```

Requires **Rust 1.95**.

## API

| Method | Path | Description |
| --- | --- | --- |
| `POST` | `/verify` | Verify a payment payload (spec §7.1) |
| `POST` | `/settle` | Settle a payment (spec §7.2) |
| `GET` | `/supported` | List supported payment kinds (version / scheme / network) |
| `GET` | `/healthz` | Liveness |
| `GET` | `/readyz` | Readiness (200 when at least one kind is registered) |
| `GET` | `/metrics` | Prometheus text; **metrics listen only**, never the protocol port |

There is no `GET /` and no `GET /health`. Optional `[http.auth]` requires `Authorization: Bearer` on `/verify`, `/settle`, and `/supported`. Ops routes stay unauthenticated.

## CLI

```text
facilitator <COMMAND>

Commands:
  init       Write config.example.toml-equivalent to PATH
  validate   Load config, construct facilitators, print /supported JSON
  serve      Bind and run

Options:
  -c, --config <PATH>   default: config.toml; env: FACILITATOR_CONFIG
```

`init --force` overwrites. `validate` and `serve` construct listed schemes. Unknown scheme **names** fail at parse; a listed scheme this build cannot construct fails at startup.

## Configuration

See [`config.example.toml`](config.example.toml) (EVM `schemes = ["exact", "upto"]`) and [`config.example.full.toml`](config.example.full.toml) (same plus SVM `exact` and `upto`).

Named `[signer.*]` plus per-network references. Literal private keys, `settlement_mode`, `[signers]`, and `[[schemes]]` are startup errors.

Env overlay: `FACILITATOR_HTTP_LISTEN`, `FACILITATOR_HTTP_METRICS_LISTEN`, `FACILITATOR_LOG_LEVEL`, `RUST_LOG`, `FACILITATOR_CONFIG`.

## Deploy

Image tag: `ghcr.io/qntx/facilitator:2.0.0`. Default features are `evm` + `svm`. Runtime is [`gcr.io/distroless/cc-debian12:nonroot`](https://github.com/GoogleContainerTools/distroless) (CA certs, no shell, no curl). There is no image `HEALTHCHECK`; probe `GET /healthz` and `GET /readyz`.

v1 is **one replica**. [`deploy/k8s/deployment.yaml`](deploy/k8s/deployment.yaml) pins `replicas: 1` and `strategy: Recreate`. Do not add HPA. `SettlementCache`, pending stores, and SVM upto channel index are process-local. Duplicate `/settle` across replicas is unsafe. Pin `:2.0.0` or a digest; do not use Watchtower or `fctl`. Tag `v*` also moves `:latest` and `:2` (org `docker/metadata-action` flavor `latest=auto`).

Drain is `settle_timeout + 5s` (default 35s): compose `stop_grace_period: 35s`, systemd `TimeoutStopSec=35`, k8s `terminationGracePeriodSeconds: 35`.

`FACILITATOR_EVM_KEY` is required for the example config. An empty key is a construct failure.

### Docker Compose (local)

[`compose.yaml`](compose.yaml) mounts [`config.example.toml`](config.example.toml) (EVM `exact`+`upto`) and overlays `FACILITATOR_HTTP_LISTEN=0.0.0.0:8080` and `FACILITATOR_HTTP_METRICS_LISTEN=0.0.0.0:9090`.

```bash
export FACILITATOR_EVM_KEY=0x...
docker compose up --build
```

SVM overlay: set `FACILITATOR_CONFIG` to [`config.example.full.toml`](config.example.full.toml) and add [`compose.svm.yaml`](compose.svm.yaml) (fee-payer key at `/run/secrets/solana.key`).

```bash
export FACILITATOR_EVM_KEY=0x...
export FACILITATOR_CONFIG=./config.example.full.toml
export SVM_FEE_PAYER_KEY=./solana.key
docker compose -f compose.yaml -f compose.svm.yaml up --build
```

### TLS (Caddy)

Optional compose profile. Edit the hostname in [`deploy/Caddyfile`](deploy/Caddyfile). Caddy does not proxy `/metrics`.

```bash
export FACILITATOR_EVM_KEY=0x...
docker compose -f deploy/compose.yaml --profile tls up --build
```

Protocol is on loopback `:8080`; metrics on loopback `:9090`; Caddy on `:80`/`:443`.

### systemd

```bash
sudo useradd --system --home /etc/facilitator --shell /usr/sbin/nologin facilitator
sudo install -D -m 0755 target/release/facilitator /usr/bin/facilitator
sudo install -d -m 0755 /etc/facilitator
sudo install -m 0600 -o facilitator -g facilitator config.toml /etc/facilitator/config.toml
sudo install -m 0644 deploy/systemd/facilitator.service /etc/systemd/system/facilitator.service
# optional: /etc/facilitator/env  (FACILITATOR_EVM_KEY=..., RUST_LOG=...)
sudo systemctl daemon-reload
sudo systemctl enable --now facilitator
```

[`deploy/systemd/facilitator.service`](deploy/systemd/facilitator.service): `Type=simple`, `Restart=on-failure`, `EnvironmentFile=-/etc/facilitator/env`, `ExecStart=/usr/bin/facilitator serve -c /etc/facilitator/config.toml`, `TimeoutStopSec=35`.

### Kubernetes

[`deploy/k8s`](deploy/k8s): `replicas: 1`, `strategy: Recreate`, probes on protocol `/healthz` and `/readyz`, protocol Service 8080 only, headless metrics Service + ServiceMonitor on pod IP:9090.

```bash
# replace facilitator-secrets.evm.key first
kubectl apply -k deploy/k8s
```

Default ConfigMap is EVM-only. SVM is opt-in (do not add the volume item until the Secret has the key — kubelet fails the mount):

1. Put a base58 fee-payer in `facilitator-secrets` as `solana.key` (see [`deploy/k8s/secret.yaml`](deploy/k8s/secret.yaml)).
2. Mount it: add `key: solana.key` / `path: solana.key` under the secrets volume `items` in [`deploy/k8s/deployment.yaml`](deploy/k8s/deployment.yaml).
3. Copy SVM tables from [`config.example.full.toml`](config.example.full.toml) into the ConfigMap (`[signer.svm_fee]` `path = "/run/secrets/solana.key"`).

Local equivalent: [`compose.svm.yaml`](compose.svm.yaml) with `FACILITATOR_CONFIG=./config.example.full.toml`.

### Image

```bash
docker build -t ghcr.io/qntx/facilitator:2.0.0 .
```

The builder clones `qntx/r402` at tag `v0.19.1` (`--build-arg R402_REF=...` to override) so the crate path deps resolve. CI publishes `ghcr.io/qntx/facilitator` on `v*` tags with buildx SBOM and provenance attestation. Pull requests and `workflow_dispatch` only prove the image builds (no push, no SBOM). metadata-action also moves `:latest` and `:2`; pin `:2.0.0` or a digest.

### Runbook

v1 ops model is one replica. `replicas: 1`, `strategy: Recreate`, no HPA. Duplicate `/settle` splits the in-memory `SettlementCache`. On listen the process logs `settlement_cache=in-memory pin_settle=true`.

Alert on:

| Signal | Meaning |
| --- | --- |
| `r402_facilitator_settle_total{result="failure"}` / `{result="error"}` | settle failed or transport/timeout |
| log `invalid_transaction_state` | Onchain reject; not HTTP 502 |
| `GET /readyz` 503 | empty constructed map / not ready |
| process down / `GET /healthz` | liveness |

Gas exhaustion is Onchain, not HTTP 502.

Rotate keys by rewriting secret files (or the Secret) and restarting. Crash-only; no hot reload.

Rollback is the previous **2.x** image plus matching config. Cannot roll to 1.0.0. 1.0.0 config will not parse (`deny_unknown_fields`). Replace `config.toml`; do not convert.

Tag `v*` publishes `ghcr.io/qntx/facilitator:2.0.0`. Org metadata-action flavor `latest=auto` also moves `:latest` and `:2`. Pin `:2.0.0` or a digest after first publish. Do not run `:latest` under Watchtower.

This crate path-depends on sibling `r402` (`../../../r402/crates/...` from `crates/facilitator`). Local builds need `qntx/r402` at tag `v0.19.1` next to this repo. The Docker builder clones that tag. The operator is not published to crates.io.

Casper is unhostable: `r402-casper` 0.19.1 `CasperExactFacilitator` is a remote HTTP client, not an on-chain facilitator. Do not proxy `casper:*`. Do not rebuild with `--features extra-casper`.

Tron is `experimental-tron` and is not in the default image.

## Feature Flags

| Feature | Default | Description |
| --- | --- | --- |
| `evm` | ✓ | Parse EIP-155 tables and construct `exact`, `upto`, `auth-capture`, `batch-settlement` |
| `svm` | ✓ | Parse Solana tables and construct `exact` and `upto` |
| `telemetry` | ✓ | Reserved for OTLP |
| `metrics` | ✓ | Enables `r402-facilitator/metrics` |
| `near` / `xrpl` / `hedera` / `avm` | | Host `exact` when the feature is on |
| `aptos` / `keeta` / `tvm` / `stellar` / `concordium` | | Compiled-out vs reserved family errors |
| `experimental-tron` | | Experimental; not in the default image |

`casper:*` is always a remote-client startup error. There is no `extra-casper` feature.

Schemes are config lists, not Cargo features. If `evm` is compiled, `exact` / `upto` / `auth-capture` / `batch-settlement` are known **names**.

## Security

See [`SECURITY.md`](SECURITY.md) for disclaimers, supported versions, and vulnerability reporting.

## Acknowledgments

- [r402](https://github.com/qntx/r402) — modular Rust SDK for the x402 payment protocol
- [x402 Protocol Specification](https://www.x402.org/) — protocol design by Coinbase
- [coinbase/x402](https://github.com/coinbase/x402) — official reference implementations (TypeScript, Python, Go)

## License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or <https://www.apache.org/licenses/LICENSE-2.0>)
- MIT License ([LICENSE-MIT](LICENSE-MIT) or <https://opensource.org/licenses/MIT>)

at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in this project shall be dual-licensed as above, without any additional terms or conditions.
