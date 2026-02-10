//! Configuration loading and default template generation.
//!
//! This module provides:
//!
//! - [`Config`] — Type alias combining the base [`r402::config::Config`] with
//!   chain-specific [`ChainsConfig`](crate::chain::ChainsConfig).
//! - [`load_config`] — Reads and parses a TOML configuration file, with
//!   automatic global-signer injection and scheme auto-generation.
//! - [`generate_default_config`] — Produces a commented TOML template.
//!
//! # Configuration File Format
//!
//! ```toml
//! host = "0.0.0.0"
//! port = 8080
//!
//! [signers]
//! evm = ["$EVM_SIGNER_PRIVATE_KEY"]
//! solana = "$SOLANA_SIGNER_PRIVATE_KEY"
//! # mnemonic = "$MNEMONIC"  # alternative: derive keys from BIP-39 phrase
//!
//! [chains."eip155:84532"]
//! rpc = [{ http = "https://sepolia.base.org" }]
//!
//! # [[schemes]] is optional — auto-generated from configured chains.
//! ```

use crate::chain::ChainsConfig;
use crate::signers;
use std::path::Path;

/// Server configuration parameterised with chain-specific config.
pub type Config = r402::config::Config<ChainsConfig>;

/// Load configuration from a TOML file at the given path.
///
/// Values not present in the file fall back to environment variables
/// (`PORT`, `HOST`) and then to hardcoded defaults.
///
/// # Errors
///
/// Returns an error if the file cannot be resolved, read, or parsed.
pub fn load_config(path: &Path) -> Result<Config, Box<dyn std::error::Error>> {
    let config_path = path
        .canonicalize()
        .map_err(|e| format!("Failed to resolve config path '{}': {e}", path.display()))?;
    let raw_content = std::fs::read_to_string(&config_path).map_err(|e| {
        format!(
            "Failed to read config file '{}': {e}",
            config_path.display()
        )
    })?;

    // Pre-process: extract [signers], inject into chains, auto-generate schemes.
    let processed = signers::preprocess_config(&raw_content).map_err(|e| {
        format!(
            "Failed to pre-process config '{}': {e}",
            config_path.display()
        )
    })?;

    let config: Config = toml::from_str(&processed).map_err(|e| {
        format!(
            "Failed to parse TOML config '{}': {e}",
            config_path.display()
        )
    })?;
    Ok(config)
}

/// Generate a default TOML configuration template.
///
/// The output includes commented sections for every chain family enabled
/// at compile time.  Uses the new simplified format with global signers
/// and optional mnemonic support.
#[must_use]
pub fn generate_default_config() -> String {
    let mut config = String::from(
        r#"# x402 Facilitator Configuration
# https://www.x402.org

# Server bind address and port.
# Can also be set via HOST / PORT environment variables.
host = "0.0.0.0"
port = 8080

# Global Signers
#
# Shared across all chains of the same type.
# Per-chain overrides are still possible (add `signers` / `signer` to
# the individual chain table).
#
# Option A: direct private keys
# Option B: BIP-39 mnemonic (keys derived via BIP-44 / SLIP-10)
#
# If both are provided, direct keys take priority over mnemonic.

[signers]
"#,
    );

    #[cfg(feature = "chain-eip155")]
    config.push_str(
        r#"evm = ["$EVM_SIGNER_PRIVATE_KEY"]       # hex, 0x-prefixed
"#,
    );

    #[cfg(feature = "chain-solana")]
    config.push_str(
        r#"solana = "$SOLANA_SIGNER_PRIVATE_KEY"    # base58, 64-byte keypair
"#,
    );

    config.push_str(
        r#"# mnemonic = "$MNEMONIC"               # BIP-39 phrase (alternative)
# passphrase = ""                       # optional BIP-39 passphrase
# evm_derivation_path = "m/44'/60'/0'/0/0"    # default MetaMask path
# solana_derivation_path = "m/44'/501'/0'/0'"  # default Phantom path
"#,
    );

    #[cfg(feature = "chain-eip155")]
    config.push_str(
        r#"
# EIP-155 (EVM) chains
#
# Key format: "eip155:<chain_id>"
# Only RPC config is needed; signers are injected from [signers] above.

[chains."eip155:84532"]
rpc = [{ http = "https://sepolia.base.org" }]
"#,
    );

    #[cfg(feature = "chain-solana")]
    config.push_str(
        r#"
# Solana chains
#
# Key format: "solana:<genesis_hash>"

[chains."solana:EtWTRABZaYq6iMfeYKouRu166VU2xqa1"]
rpc = "https://api.devnet.solana.com"
"#,
    );

    config.push_str(
        r#"
# Scheme registrations (optional)
#
# If omitted, all configured chains are auto-registered with
# every available scheme version (v1 + v2).
#
# Uncomment below only if you need to restrict schemes:
#
# [[schemes]]
# scheme = "v2-eip155-exact"
# chains = ["eip155:84532"]
"#,
    );

    config
}
