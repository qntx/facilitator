# x402 Facilitator

A production-ready HTTP server implementing the [x402](https://www.x402.org) payment protocol for blockchain-based micropayments.

## Overview

The facilitator is a trusted third party that verifies and settles payments on behalf of resource servers. It does not hold funds — it only validates payment signatures and broadcasts settlement transactions on-chain.

## Endpoints

| Method | Path | Description |
| -------- | ------ | ------------- |
| `GET` | `/supported` | List supported payment kinds (version/scheme/network) |
| `POST` | `/verify` | Verify a payment payload against requirements |
| `POST` | `/settle` | Settle an accepted payment on-chain |
| `GET` | `/health` | Health check |

## Supported Chains

- **EVM (EIP-155)**: Base, Ethereum, Polygon, Avalanche, Celo, and more
- **Solana (SVM)**: Mainnet, Devnet, and custom clusters

## Quick Start

```bash
# Build
cargo build --release

# Run with config
./target/release/facilitator --config config.json

# Docker
docker build -t x402-facilitator .
docker run -p 4021:4021 -v ./config.json:/app/config.json x402-facilitator
```

## Configuration

The server loads configuration from a JSON file specified by `--config` or the `CONFIG` environment variable.

### Environment Variables

| Variable | Default | Description |
| --------- | --------- | ------------- |
| `HOST` | `0.0.0.0` | Bind address |
| `PORT` | `8080` | Listen port |
| `CONFIG` | `config.json` | Config file path |
| `OTEL_*` | — | OpenTelemetry configuration |

## Features

| Feature | Default | Description |
| --------- | --------- | ------------- |
| `chain-eip155` | ✓ | EVM chain support |
| `chain-solana` | ✓ | Solana chain support |
| `telemetry` | ✓ | OpenTelemetry tracing and metrics |

## Built with

- [r402](https://crates.io/crates/r402) — x402 Payment Protocol SDK for Rust
- [r402-evm](https://crates.io/crates/r402-evm) — EVM chain implementation
- [r402-svm](https://crates.io/crates/r402-svm) — Solana chain implementation

## License

This project is licensed under either of the following licenses, at your option:

- Apache License, Version 2.0, ([LICENSE-APACHE](LICENSE-APACHE) or [https://www.apache.org/licenses/LICENSE-2.0](https://www.apache.org/licenses/LICENSE-2.0))
- MIT license ([LICENSE-MIT](LICENSE-MIT) or [https://opensource.org/licenses/MIT](https://opensource.org/licenses/MIT))

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in this project by you, as defined in the Apache-2.0 license, shall be dually licensed as above, without any additional terms or conditions.
