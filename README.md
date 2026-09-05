<!-- markdownlint-disable MD033 MD041 MD036 -->

# Facilitator

[![Crates.io][crates-badge]][crates-url]
[![Docs.rs][docs-badge]][docs-url]
[![CI][ci-badge]][ci-url]
[![License][license-badge]][license-url]
[![Rust][rust-badge]][rust-url]

[crates-badge]: https://img.shields.io/crates/v/facilitator.svg
[crates-url]: https://crates.io/crates/facilitator
[docs-badge]: https://img.shields.io/docsrs/facilitator.svg
[docs-url]: https://docs.rs/facilitator
[ci-badge]: https://github.com/qntx/facilitator/actions/workflows/ci.yml/badge.svg
[ci-url]: https://github.com/qntx/facilitator/actions/workflows/ci.yml
[license-badge]: https://img.shields.io/badge/license-MIT%2FApache--2.0-blue.svg
[license-url]: LICENSE-MIT
[rust-badge]: https://img.shields.io/badge/rust-edition%202024-orange.svg
[rust-url]: https://doc.rust-lang.org/edition-guide/

**[x402](https://www.x402.org/) V2 facilitator** — verify payment payloads and settle on-chain over HTTP.

Built on [r402](https://github.com/qntx/r402) **0.20**. Spec §7 (`POST /verify`, `POST /settle`, `GET /supported`) plus `/healthz`, `/readyz`, and an optional metrics listen. Default image hosts EVM `exact` / `upto` / `auth-capture` / `batch-settlement` and SVM `exact` / `upto`. One replica; settlement state is process-local.

Casper cannot be hosted (`r402-casper` is a remote HTTP client). Tron is `experimental-tron` and is not in the default image.

## Quick Start

```bash
cargo install facilitator
facilitator init
# edit config.toml — named [signer.*], env or file secrets only
facilitator serve
```

Requires **Rust 1.95**.

```bash
export FACILITATOR_EVM_KEY=0x...
export FACILITATOR_API_TOKEN=...
docker compose up --build
# Orb: docker-compose up --build
```

Image: `ghcr.io/qntx/facilitator:0.7.1`. Pin that tag or a digest — not `:latest` / `:0` / `:0.7`. Do not run `:0.7.0` (USER 65532 cannot traverse `/etc/facilitator`).

## API

| Method | Path | Description |
| --- | --- | --- |
| `POST` | `/verify` | Verify a payment payload (§7.1) |
| `POST` | `/settle` | Settle a payment (§7.2) |
| `GET` | `/supported` | Supported kinds |
| `GET` | `/healthz` | Liveness |
| `GET` | `/readyz` | Ready when at least one kind is registered |
| `GET` | `/metrics` | Prometheus text; **metrics listen only** |

No `GET /`. `[http.auth]` bearer applies to `/verify`, `/settle`, `/supported`. Required when `http.listen` is not loopback (Docker/k8s `0.0.0.0` overlay included). Metrics listen may be `0.0.0.0` without bearer. Ops routes stay unauthenticated.

```text
facilitator init | validate | serve
  -c, --config <PATH>   default: config.toml; env: FACILITATOR_CONFIG
```

`validate` and `serve` construct listed schemes. Unknown scheme names fail at parse; a listed scheme this build cannot construct fails at startup.

## Configuration

[`config.example.toml`](config.example.toml) (EVM) and [`config.example.full.toml`](config.example.full.toml) (EVM + SVM). Named `[signer.*]` plus per-network references. Literal private keys are a startup error.

```toml
[http.auth]
bearer_env = "FACILITATOR_API_TOKEN"

[signer.evm_hot]
source = "env"
env = "FACILITATOR_EVM_KEY"

[network."eip155:84532"]
rpc = ["https://sepolia.base.org"]
signers = ["evm_hot"]
schemes = ["exact", "upto"]
```

Env overlay: `FACILITATOR_HTTP_LISTEN`, `FACILITATOR_HTTP_METRICS_LISTEN`, `FACILITATOR_LOG_LEVEL`, `RUST_LOG`, `FACILITATOR_CONFIG`. Overlay re-validates: non-loopback listen without `[http.auth]` is a startup error.

## Features

| Feature | Default | |
| --- | --- | --- |
| `evm` | ✓ | EIP-155 `exact` / `upto` / `auth-capture` / `batch-settlement` |
| `svm` | ✓ | Solana `exact` / `upto` |
| `telemetry` / `metrics` | ✓ | |
| `near` `xrpl` `hedera` `avm` `aptos` `keeta` `tvm` `stellar` `concordium` | | `exact` |
| `experimental-tron` | | `exact`; not in the default image |

Schemes are config lists, not Cargo features.

## Deploy

Runtime is [`gcr.io/distroless/cc-debian12:nonroot`](https://github.com/GoogleContainerTools/distroless) (USER **65532**). Probe `GET /healthz` and `GET /readyz`. Distroless has no `HEALTHCHECK`. Drain is `settle_timeout + 5s` (default 35s).

- Image is **linux/amd64** only. Apple Silicon / Orb: `platform: linux/amd64` is set in compose (QEMU). Build with `docker buildx` (Orb: `docker-buildx`), not legacy `docker build`. Probe with **curl**, not Python urllib, under QEMU.
- Never `--user 0`. Image USER is 65532.
- Compose file-mount of `config.toml` requires the host file readable by uid 65532 (`config.example.toml` is 0644). Host `0600` is `EACCES` inside the container.
- Protocol `:8080` and metrics `:9090` are separate. Do not publish them on a public interface. [`deploy/compose.yaml`](deploy/compose.yaml) binds host `127.0.0.1`. Put Caddy (or equivalent) in front. In-container listen is `0.0.0.0`, so `[http.auth]` and `FACILITATOR_API_TOKEN` are required.
- One replica. Settlement cache is in-memory. Do not `--scale`. k8s `strategy: Recreate`.
- Secrets: `FACILITATOR_EVM_KEY` or a file source, plus `FACILITATOR_API_TOKEN`. Never TOML literals (startup error).
- Pin `ghcr.io/qntx/facilitator:0.7.1` or a digest. `:0.7.0` is known-bad.

Compose: [`compose.yaml`](compose.yaml) (`docker compose` / Orb `docker-compose`), SVM overlay [`compose.svm.yaml`](compose.svm.yaml), TLS profile [`deploy/compose.yaml`](deploy/compose.yaml).

systemd: [`deploy/systemd/facilitator.service`](deploy/systemd/facilitator.service) is a **host** unit (`User=facilitator`), not distroless uid 65532.

```bash
install -d -m 0755 /etc/facilitator
install -m 0644 /path/to/config.toml /etc/facilitator/config.toml
```

Kubernetes: [`deploy/k8s`](deploy/k8s) — `replicas: 1`, `strategy: Recreate`, `runAsUser: 65532`, ConfigMap **directory** mount at `/etc/facilitator` (not a `subPath` file mount).

## Acknowledgments

- [r402](https://github.com/qntx/r402) — modular Rust SDK for the x402 payment protocol
- [x402 Protocol Specification](https://www.x402.org/) — protocol design by Coinbase
- [coinbase/x402](https://github.com/coinbase/x402) — official reference implementations

## License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or <https://www.apache.org/licenses/LICENSE-2.0>)
- MIT License ([LICENSE-MIT](LICENSE-MIT) or <https://opensource.org/licenses/MIT>)

at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in this project shall be dual-licensed as above, without any additional terms or conditions.

---

<div align="center">

A **[QuantX](https://qntx.org)** open-source project.

<a href="https://qntx.org"><img alt="QuantX" width="369" src="https://raw.githubusercontent.com/qntx/.github/main/profile/qntx.svg" /></a>

Code is law. We write both.

</div>
