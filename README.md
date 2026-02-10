# x402 Facilitator

[![Crates.io](https://img.shields.io/crates/v/facilitator.svg)](https://crates.io/crates/facilitator)
[![License](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue.svg)](#license)

A production-ready CLI tool and HTTP server implementing the [x402](https://www.x402.org) payment protocol for blockchain-based micropayments.

## Overview

The facilitator is a trusted third party that verifies and settles payments on behalf of resource servers. It does not hold funds — it only validates payment signatures and broadcasts settlement transactions on-chain.

## Quick Start

```bash
# Build
cargo build --release

# Generate a default config file
facilitator init

# Edit config.toml with your RPC URLs and signer keys, then start
facilitator serve
facilitator serve --config my-config.toml
```

### Docker

```bash
docker build -t facilitator .
docker run -p 8080:8080 -v ./config.toml:/app/config.toml facilitator

# Build with specific chain features only
docker build -t facilitator --build-arg FEATURES=chain-eip155 .
```

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

The server loads configuration from a TOML file (default: `config.toml`). Run `facilitator init` to generate a commented template.

```toml
host = "0.0.0.0"
port = 8080

[chains."eip155:84532"]
rpc_url = "https://sepolia.base.org"
signer_private_key = "$EIP155_SIGNER_PRIVATE_KEY"

[[schemes]]
scheme = "v2-eip155-exact"
chains = ["eip155:84532"]
```

### Environment Variables

| Variable | Default       | Description                    |
| -------- | ------------- | ------------------------------ |
| `HOST`   | `0.0.0.0`     | Bind address                   |
| `PORT`   | `8080`        | Listen port                    |
| `CONFIG` | `config.toml` | Config file path (for `serve`) |
| `OTEL_*` | —             | OpenTelemetry configuration    |

## Endpoints

| Method | Path         | Description                                           |
| ------ | ------------ | ----------------------------------------------------- |
| `GET`  | `/supported` | List supported payment kinds (version/scheme/network) |
| `POST` | `/verify`    | Verify a payment payload against requirements         |
| `POST` | `/settle`    | Settle an accepted payment on-chain                   |
| `GET`  | `/health`    | Health check                                          |

## Supported Chains

- **EVM (EIP-155)** — Base, Ethereum, Polygon, Avalanche, Celo, and more
- **Solana (SVM)** — Mainnet, Devnet, and custom clusters

## Features

| Feature        | Default | Description                       |
| -------------- | ------- | --------------------------------- |
| `chain-eip155` | ✓       | EVM chain support                 |
| `chain-solana` | ✓       | Solana chain support              |
| `telemetry`    | ✓       | OpenTelemetry tracing and metrics |

## Built With

- [r402](https://crates.io/crates/r402) — x402 Payment Protocol SDK for Rust
- [r402-evm](https://crates.io/crates/r402-evm) — EVM chain implementation
- [r402-svm](https://crates.io/crates/r402-svm) — Solana chain implementation

## License

This project is licensed under either of the following licenses, at your option:

- Apache License, Version 2.0, ([LICENSE-APACHE](LICENSE-APACHE) or [https://www.apache.org/licenses/LICENSE-2.0](https://www.apache.org/licenses/LICENSE-2.0))
- MIT license ([LICENSE-MIT](LICENSE-MIT) or [https://opensource.org/licenses/MIT](https://opensource.org/licenses/MIT))

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in this project by you, as defined in the Apache-2.0 license, shall be dually licensed as above, without any additional terms or conditions.
