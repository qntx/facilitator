# Facilitator

[![CI][ci-badge]][ci-url]
[![Crates.io][crate-badge]][crate-url]
[![Docker][docker-badge]][docker-url]
[![License][license-badge]][license-url]
[![Rust][rust-badge]][rust-url]

[ci-badge]: https://github.com/qntx/facilitator/actions/workflows/rust.yml/badge.svg
[ci-url]: https://github.com/qntx/facilitator/actions/workflows/rust.yml
[crate-badge]: https://img.shields.io/crates/v/facilitator.svg
[crate-url]: https://crates.io/crates/facilitator
[docker-badge]: https://img.shields.io/badge/ghcr.io-facilitator-blue
[docker-url]: https://github.com/qntx/facilitator/pkgs/container/facilitator
[license-badge]: https://img.shields.io/badge/license-MIT%2FApache--2.0-blue.svg
[license-url]: LICENSE-MIT
[rust-badge]: https://img.shields.io/badge/rust-edition%202024-orange.svg
[rust-url]: https://doc.rust-lang.org/edition-guide/

**Production-ready [x402 payment protocol](https://www.x402.org/) facilitator — verifies payment signatures and settles transactions on-chain over HTTP 402.**

The facilitator is a trusted third party that acts on behalf of resource servers. It does not hold funds — it only validates payment payloads and broadcasts settlement transactions to the blockchain.

Built on [r402](https://github.com/qntx/r402), the modular Rust SDK for x402.

> [!WARNING]
> This software has **not** been audited. See [Security](#security) before using in production.

## Quick Start

```bash
# Install from crates.io
cargo install facilitator

# Generate a commented config template
facilitator init

# Edit config.toml with your RPC URLs and signer keys, then start
facilitator serve
```

### Docker

```bash
# Using pre-built image
docker run -p 8080:8080 -v ./config.toml:/app/config.toml ghcr.io/qntx/facilitator

# Or build from source
docker build -t facilitator .
docker build -t facilitator --build-arg FEATURES=chain-eip155 .   # EVM only
docker run -p 8080:8080 -v ./config.toml:/app/config.toml facilitator
```

## API

| Method | Path | Description |
| --- | --- | --- |
| `GET` | `/supported` | List supported payment kinds (version / scheme / network) |
| `POST` | `/verify` | Verify a payment payload against requirements |
| `POST` | `/settle` | Settle an accepted payment on-chain |
| `GET` | `/health` | Health check |

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

The server loads configuration from a TOML file (default: `config.toml`). Run `facilitator init` to generate a fully commented template.

```toml
host = "0.0.0.0"
port = 8080

# Global signers — shared across all chains of the same type.
# Env-var references ("$VAR" or "${VAR}") are resolved at startup.
[signers]
evm    = ["$EVM_SIGNER_PRIVATE_KEY"]       # hex, 0x-prefixed
solana = "$SOLANA_SIGNER_PRIVATE_KEY"       # base58, 64-byte keypair

# EVM chains (CAIP-2 key format: "eip155:<chain_id>")
[chains."eip155:8453"]
rpc = [{ http = "https://mainnet.base.org" }]

[chains."eip155:84532"]
rpc = [{ http = "https://sepolia.base.org" }]

# Solana chains
[chains."solana:5eykt4UsFv8P8NJdTREpY1vzqKqZKvdp"]
rpc = "https://api.mainnet-beta.solana.com"

# Scheme registrations (optional — auto-generated from configured chains)
# [[schemes]]
# id = "v2-eip155-exact"
# chains = "eip155:{8453,84532}"
```

### Environment Variables

| Variable | Default | Description |
| --- | --- | --- |
| `HOST` | `0.0.0.0` | Bind address |
| `PORT` | `8080` | Listen port |
| `CONFIG` | `config.toml` | Config file path (for `serve`) |
| `OTEL_*` | — | OpenTelemetry configuration |

## Supported Chains

| Family | Networks |
| --- | --- |
| **EVM (EIP-155)** | Ethereum, Base, Optimism, Arbitrum, Polygon, Avalanche, Celo, Monad, and testnets |
| **Solana (SVM)** | Mainnet, Devnet, and custom clusters |

## Feature Flags

| Feature | Default | Description |
| --- | --- | --- |
| `chain-eip155` | ✓ | EVM chain support via [r402-evm](https://crates.io/crates/r402-evm) |
| `chain-solana` | ✓ | Solana chain support via [r402-svm](https://crates.io/crates/r402-svm) |
| `telemetry` | ✓ | OpenTelemetry tracing and metrics |

Disable unused chains to reduce binary size and compile time:

```bash
cargo install facilitator --no-default-features --features chain-eip155
```

## Security

> [!CAUTION]
> **This software has NOT been audited by any independent security firm.**

This service interacts with blockchain networks and processes real financial transactions. Bugs or vulnerabilities **may result in irreversible loss of funds**.

- **No warranty.** Provided "AS IS" without warranty of any kind, express or implied.
- **Unaudited.** The codebase has not undergone a formal security audit.
- **Testnet first.** Always validate on testnets before deploying to mainnet.
- **Key management.** Users are solely responsible for the secure handling of private keys and signing credentials. Never commit secrets to version control — use environment variable references in your config.

To report a vulnerability, please open a [GitHub Security Advisory](https://github.com/qntx/facilitator/security/advisories/new) — do not file a public issue.

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
