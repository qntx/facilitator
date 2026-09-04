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

This skeleton parses the 2.0 config schema and serves `GET /supported` from an in-process map that may be empty. EVM/SVM constructors, `/verify`, `/settle`, health, metrics, and deploy artifacts land in later commits.

## Quick Start

```bash
facilitator init
# edit config.toml — named [signer.*] tables, env/file secrets only
facilitator validate -c config.toml
facilitator serve -c config.toml
```

Requires **Rust 1.95**.

## API

| Method | Path | Description |
| --- | --- | --- |
| `GET` | `/supported` | List supported payment kinds (version / scheme / network) |

`POST /verify` and `POST /settle` are not bound in this skeleton. There is no `GET /` and no `GET /health`.

## CLI

```text
facilitator <COMMAND>

Commands:
  init       Write config.example.toml-equivalent to PATH
  validate   Load config, resolve secrets, print network/scheme list
  serve      Bind and run

Options:
  -c, --config <PATH>   default: config.toml; env: FACILITATOR_CONFIG
```

`init --force` overwrites. `validate` does not construct scheme facilitators: unknown scheme **names** fail; unimplemented constructors do not.

## Configuration

See [`config.example.toml`](config.example.toml) (EVM `schemes = ["exact"]`) and [`config.example.full.toml`](config.example.full.toml) (SVM tables as documentation).

Named `[signer.*]` plus per-network references. Literal private keys, `settlement_mode`, `[signers]`, and `[[schemes]]` are startup errors.

Env overlay: `FACILITATOR_HTTP_LISTEN`, `FACILITATOR_HTTP_METRICS_LISTEN`, `FACILITATOR_LOG_LEVEL`, `RUST_LOG`, `FACILITATOR_CONFIG`.

## Feature Flags

| Feature | Default | Description |
| --- | --- | --- |
| `evm` | ✓ | Parse EIP-155 network tables (constructors later) |
| `svm` | ✓ | Parse Solana network tables (constructors later) |
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
