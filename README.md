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

v1 is a **single replica**. In-memory stores are process-local. Enabling `batch-settlement` logs `batch-settlement requires a single replica` at startup. Deploy artifacts land in a later commit.

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

## Feature Flags

| Feature | Default | Description |
| --- | --- | --- |
| `evm` | ✓ | Parse EIP-155 tables and construct `exact`, `upto`, `auth-capture`, `batch-settlement` |
| `svm` | ✓ | Parse Solana tables and construct `exact` and `upto` |
| `telemetry` | ✓ | Reserved for OTLP |
| `metrics` | ✓ | Enables `r402-facilitator/metrics` |
| `near` / `xrpl` / `hedera` / `avm` / `aptos` / `keeta` / `tvm` / `stellar` / `concordium` | | Compiled-out vs reserved family errors |
| `experimental-tron` / `extra-casper` | | Rejected unless the feature is on |

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
