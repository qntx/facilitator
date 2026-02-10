//! Configuration loading and default template generation.
//!
//! This module provides:
//!
//! - [`Config`] — Type alias combining the base [`r402::config::Config`] with
//!   chain-specific [`ChainsConfig`](crate::chain::ChainsConfig).
//! - [`load_config`] — Reads and parses a TOML configuration file.
//! - [`generate_default_config`] — Produces a commented TOML template.
//!
//! # Configuration File Format
//!
//! ```toml
//! port = 8080
//! host = "0.0.0.0"
//!
//! [chains."eip155:84532"]
//! rpc_url = "https://sepolia.base.org"
//! signer_private_key = "$EIP155_SIGNER_PRIVATE_KEY"
//!
//! [[schemes]]
//! scheme = "v2-eip155-exact"
//! chains = ["eip155:84532"]
//! ```

use crate::chain::ChainsConfig;
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
    let content = std::fs::read_to_string(&config_path).map_err(|e| {
        format!(
            "Failed to read config file '{}': {e}",
            config_path.display()
        )
    })?;
    let config: Config = toml::from_str(&content).map_err(|e| {
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
/// at compile time.
#[must_use]
pub fn generate_default_config() -> String {
    let mut config = String::from(
        r#"# x402 Facilitator Configuration
# https://www.x402.org

# Server bind address and port.
# Can also be set via HOST / PORT environment variables.
host = "0.0.0.0"
port = 8080
"#,
    );

    #[cfg(feature = "chain-eip155")]
    config.push_str(
        r#"
# ── EIP-155 (EVM) chains ────────────────────────────────────────────
# Key format: "eip155:<chain_id>"
# Values support environment variable references: "$VAR" or "${VAR}"

[chains."eip155:84532"]
rpc_url = "https://sepolia.base.org"
signer_private_key = "$EIP155_SIGNER_PRIVATE_KEY"
"#,
    );

    #[cfg(feature = "chain-solana")]
    config.push_str(
        r#"
# ── Solana chains ────────────────────────────────────────────────────
# Key format: "solana:<genesis_hash>"

[chains."solana:EtWTRABZaYq6iMfeYKouRu166VU2xqa1"]
rpc_url = "https://api.devnet.solana.com"
signer_private_key = "$SOLANA_SIGNER_PRIVATE_KEY"
"#,
    );

    #[cfg(feature = "chain-eip155")]
    config.push_str(
        r#"
# ── Scheme registrations ────────────────────────────────────────────
# Each [[schemes]] entry enables a payment scheme on specific chains.

[[schemes]]
scheme = "v1-eip155-exact"
chains = ["eip155:84532"]

[[schemes]]
scheme = "v2-eip155-exact"
chains = ["eip155:84532"]
"#,
    );

    #[cfg(feature = "chain-solana")]
    config.push_str(
        r#"
[[schemes]]
scheme = "v1-solana-exact"
chains = ["solana:EtWTRABZaYq6iMfeYKouRu166VU2xqa1"]

[[schemes]]
scheme = "v2-solana-exact"
chains = ["solana:EtWTRABZaYq6iMfeYKouRu166VU2xqa1"]
"#,
    );

    config
}
