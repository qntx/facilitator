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

use std::collections::BTreeMap;
use std::net::IpAddr;
use std::path::Path;

use r402::chain::ChainIdPattern;
use serde::{Deserialize, Serialize};

use crate::chain::ChainsConfig;
use crate::error::Error;
use crate::signers;

/// Scheme registration entry from the TOML config.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchemeEntry {
    /// Scheme identifier (e.g. "v2-eip155-exact").
    pub id: String,
    /// Chain pattern (e.g. "eip155:*").
    pub chains: ChainIdPattern,
    /// Optional scheme-specific configuration.
    #[serde(flatten)]
    pub config: Option<serde_json::Value>,
}

/// Server configuration combining host/port, chain configs, and scheme registrations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Bind address (default: 0.0.0.0).
    #[serde(default = "default_host")]
    host: IpAddr,
    /// Listen port (default: 8080).
    #[serde(default = "default_port")]
    port: u16,
    /// Chain provider configurations keyed by CAIP-2 identifier.
    #[serde(default)]
    chains: ChainsConfig,
    /// Scheme registrations (optional, auto-generated if absent).
    #[serde(default)]
    schemes: Vec<SchemeEntry>,
}

const fn default_host() -> IpAddr {
    IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED)
}

fn default_port() -> u16 {
    std::env::var("PORT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(8080)
}

impl Config {
    /// Returns the configured bind address.
    #[must_use]
    pub const fn host(&self) -> IpAddr {
        self.host
    }

    /// Returns the configured listen port.
    #[must_use]
    pub const fn port(&self) -> u16 {
        self.port
    }

    /// Returns a reference to the chain configurations.
    #[must_use]
    pub const fn chains(&self) -> &ChainsConfig {
        &self.chains
    }

    /// Returns a reference to the scheme registrations.
    #[must_use]
    pub fn schemes(&self) -> &[SchemeEntry] {
        &self.schemes
    }
}

/// Load configuration from a TOML file at the given path.
///
/// Values not present in the file fall back to environment variables
/// (`PORT`, `HOST`) and then to hardcoded defaults.
///
/// # Errors
///
/// Returns an error if the file cannot be resolved, read, or parsed.
pub fn load_config(path: &Path) -> Result<Config, Error> {
    let config_path = path
        .canonicalize()
        .map_err(|e| Error::Config(format!("Failed to resolve '{}': {e}", path.display())))?;
    let raw_content = std::fs::read_to_string(&config_path)
        .map_err(|e| Error::Config(format!("Failed to read '{}': {e}", config_path.display())))?;

    let mut doc: BTreeMap<String, toml::Value> = toml::from_str(&raw_content)
        .map_err(|e| Error::Config(format!("Failed to parse '{}': {e}", config_path.display())))?;

    // Step 1: resolve signers and inject into chain entries
    signers::preprocess_signers(&mut doc)?;

    // Step 2: auto-generate [[schemes]] if absent
    auto_generate_schemes(&mut doc);

    let processed = toml::to_string(&doc)
        .map_err(|e| Error::Config(format!("Failed to serialize config: {e}")))?;
    let config: Config = toml::from_str(&processed)
        .map_err(|e| Error::Config(format!("Failed to parse config: {e}")))?;
    Ok(config)
}

/// Auto-generate `[[schemes]]` entries from configured chains when the section
/// is absent or empty.
fn auto_generate_schemes(doc: &mut BTreeMap<String, toml::Value>) {
    let needs_schemes = match doc.get("schemes") {
        None => true,
        Some(toml::Value::Array(arr)) => arr.is_empty(),
        _ => false,
    };
    if !needs_schemes {
        return;
    }

    let has_evm = has_chain_namespace(doc, "eip155:");
    let has_solana = has_chain_namespace(doc, "solana:");
    let mut schemes = Vec::new();

    #[cfg(feature = "chain-eip155")]
    if has_evm {
        schemes.push(scheme_entry("eip155-exact", "eip155:*"));
    }

    #[cfg(feature = "chain-solana")]
    if has_solana {
        schemes.push(scheme_entry("solana-exact", "solana:*"));
    }

    if !schemes.is_empty() {
        doc.insert("schemes".to_owned(), toml::Value::Array(schemes));
    }
}

/// Check if any chain key starts with the given namespace prefix.
fn has_chain_namespace(doc: &BTreeMap<String, toml::Value>, prefix: &str) -> bool {
    doc.get("chains")
        .and_then(|v| v.as_table())
        .is_some_and(|chains| chains.keys().any(|k| k.starts_with(prefix)))
}

/// Build a single `[[schemes]]` TOML table entry.
fn scheme_entry(id: &str, chains: &str) -> toml::Value {
    let mut entry = toml::map::Map::new();
    entry.insert("id".to_owned(), toml::Value::String(id.to_owned()));
    entry.insert("chains".to_owned(), toml::Value::String(chains.to_owned()));
    toml::Value::Table(entry)
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
# every available scheme.
#
# Uncomment below only if you need to restrict schemes:
#
# [[schemes]]
# id = "eip155-exact"
# chains = "eip155:84532"
"#,
    );

    config
}
