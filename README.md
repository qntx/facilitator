# Facilitator

[![CI][ci-badge]][ci-url]
[![Crates.io][crate-badge]][crate-url]
[![Docker][docker-badge]][docker-url]
[![License][license-badge]][license-url]
[![Rust][rust-badge]][rust-url]

[ci-badge]: https://github.com/qntx/facilitator/actions/workflows/ci.yml/badge.svg
[ci-url]: https://github.com/qntx/facilitator/actions/workflows/ci.yml
[crate-badge]: https://img.shields.io/crates/v/facilitator.svg
[crate-url]: https://crates.io/crates/facilitator
[docker-badge]: https://img.shields.io/badge/ghcr.io-facilitator-blue
[docker-url]: https://github.com/qntx/facilitator/pkgs/container/facilitator
[license-badge]: https://img.shields.io/badge/license-MIT%2FApache--2.0-blue.svg
[license-url]: #license
[rust-badge]: https://img.shields.io/badge/rust-1.95%20%2B%20edition%202024-orange.svg
[rust-url]: https://doc.rust-lang.org/edition-guide/

**[x402](https://www.x402.org/) V2 facilitator process** — verifies payment payloads and settles transactions on-chain over HTTP.

The facilitator is a trusted third party that acts on behalf of resource servers. It does not hold funds — it only validates payment payloads and broadcasts settlement transactions to the blockchain.

Built on [r402](https://github.com/qntx/r402) **0.17.1**. This 1.0.0 process ships **EIP-155 exact** only; Solana and extra EVM schemes land in later PRs. See [Security](SECURITY.md) before using in production.

## Quick Start

```bash
# Install from crates.io
cargo install facilitator

# Generate a commented config template
facilitator init

# Edit config.toml with your RPC URLs and signer keys, then start
facilitator serve
```

Requires **Rust 1.95**.

### Docker

```bash
# Using pre-built image
docker run -p 8080:8080 -v ./config.toml:/app/config.toml ghcr.io/qntx/facilitator

# Or build from source
docker build -t facilitator .
docker build -t facilitator --build-arg FEATURES=chain-eip155,telemetry .
docker run -p 8080:8080 -v ./config.toml:/app/config.toml facilitator
```

## API

| Method | Path | Description |
| --- | --- | --- |
| `GET` | `/supported` | List supported payment kinds (version / scheme / network) |
| `POST` | `/verify` | Verify a payment payload against requirements |
| `POST` | `/settle` | Settle an accepted payment on-chain |
| `GET` | `/health` | Process liveness (not part of the x402 protocol) |

There is no `GET /`. Protocol verify/settle outcomes are HTTP 200 with structured JSON (`isValid` / `success`). HTTP 400 is only returned for an unparseable body.

## CLI

```text
facilitator <COMMAND>

Commands:
  init   Generate a default TOML configuration file
  serve  Start the facilitator HTTP server

Options:
  -h, --help     Print help
  -V, --version  Print version
```

### `init`

```text
facilitator init [OPTIONS]

Options:
  -o, --output <PATH>  Output path [default: config.toml]
      --force          Overwrite existing file
```

### `serve`

```text
facilitator serve [OPTIONS]

Options:
  -c, --config <PATH>  Path to TOML config file [default: config.toml]
```

## Configuration

The server loads configuration from a TOML file (default: `config.toml`). Run `facilitator init` to generate a template for families compiled into the binary.

```toml
host = "0.0.0.0"
port = 8080
log_level = "info"

[signers]
evm = ["$EVM_SIGNER_PRIVATE_KEY"]       # hex, 0x-prefixed

[chains."eip155:84532"]
rpc = [{ http = "https://sepolia.base.org" }]
receipt_timeout_secs = 20
```

Do **not** add `[[schemes]]`. Schemes are compile-time (`chain-eip155` registers EVM exact). A `schemes` key is a startup error.

Empty `[chains]`, an unknown CAIP-2 namespace, or a family not compiled into the binary is a startup error.

HTTP timeouts: 30 s on `/verify`, `/settle`, and `/supported`; 5 s on `/health`. Keep `receipt_timeout_secs` below the 30 s client budget (default 20).

### Environment Variables

| Variable | Default | Description |
| --- | --- | --- |
| `HOST` | `0.0.0.0` | Bind address |
| `PORT` | `8080` | Listen port |
| `CONFIG` | `config.toml` | Config file path (for `serve`) |
| `OTEL_*` | — | OpenTelemetry configuration |

## Supported Chains

| Family | This build | Notes |
| --- | --- | --- |
| **EVM (EIP-155) exact** | default | Ethereum, Base, and any `eip155:<id>` with RPC + signer |
| **Solana (SVM)** | later PR | Not compiled in 1.0.0-pr1 default features |

## Feature Flags

| Feature | Default | Description |
| --- | --- | --- |
| `chain-eip155` | ✓ | EVM exact via [r402-evm](https://crates.io/crates/r402-evm) 0.17.1 |
| `telemetry` | ✓ | OpenTelemetry tracing and metrics |
| `metrics` | | Process HTTP `facilitator_http_*` via the [`metrics`](https://docs.rs/metrics/0.24) facade; enables `r402-core/metrics` for `r402_settlement_cache_reserve_total` |

### Metrics (`--features metrics`)

The binary does **not** install a recorder and does not bind Prometheus. Operators attach one (for example `metrics-exporter-prometheus`). `telemetry` OTLP (`MetricsLayer`) does not scrape this facade.

| Name | `result` |
| --- | --- |
| `facilitator_http_verify_total` | `valid` \| `invalid` \| `error` |
| `facilitator_http_verify_duration_seconds` | same |
| `facilitator_http_settle_total` | `success` \| `failure` \| `error` |
| `facilitator_http_settle_duration_seconds` | same |

`error` is HTTP 400, cancelled/504 timeout, and `FacilitatorError` other than a missing handler. Envelope rejects and `no_facilitator_for_network` are `invalid` / `failure`. This process never increments `r402_facilitator_*`.

```bash
cargo install facilitator --no-default-features --features chain-eip155
```

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

---

<div align="center">

A **[QuantX](https://qntx.org)** open-source project.

<a href="https://qntx.org"><img alt="QuantX" width="369" src="https://raw.githubusercontent.com/qntx/.github/main/profile/qntx.svg" /></a>

Code is law. We write both.

</div>
