//! Configuration loading and default template generation.
//!
//! This module provides:
//!
//! - [`Config`] — Type alias combining the base [`r402::config::Config`] with
//!   chain-specific [`ChainsConfig`](crate::chain::ChainsConfig).
//! - [`load_config`] — Reads and parses a TOML configuration file, with
//!   automatic global-signer injection and scheme auto-generation.
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
        .map_err(|e| Error::config_with(format!("failed to resolve '{}'", path.display()), e))?;
    let raw_content = std::fs::read_to_string(&config_path).map_err(|e| {
        Error::config_with(format!("failed to read '{}'", config_path.display()), e)
    })?;

    let mut doc: BTreeMap<String, toml::Value> = toml::from_str(&raw_content).map_err(|e| {
        Error::config_with(format!("failed to parse '{}'", config_path.display()), e)
    })?;

    // Step 1: resolve signers and inject into chain entries
    signers::preprocess_signers(&mut doc)?;

    // Step 2: auto-generate [[schemes]] if absent
    auto_generate_schemes(&mut doc);

    let processed =
        toml::to_string(&doc).map_err(|e| Error::config_with("failed to serialize config", e))?;
    let config: Config =
        toml::from_str(&processed).map_err(|e| Error::config_with("failed to parse config", e))?;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn has_chain_namespace_matches_eip155() {
        let mut doc: BTreeMap<String, toml::Value> = BTreeMap::new();
        let mut chains = toml::map::Map::new();
        chains.insert(
            "eip155:84532".into(),
            toml::Value::Table(toml::map::Map::new()),
        );
        doc.insert("chains".into(), toml::Value::Table(chains));

        assert!(has_chain_namespace(&doc, "eip155:"));
        assert!(!has_chain_namespace(&doc, "solana:"));
    }

    #[test]
    fn has_chain_namespace_no_chains() {
        let doc: BTreeMap<String, toml::Value> = BTreeMap::new();
        assert!(!has_chain_namespace(&doc, "eip155:"));
    }

    #[test]
    fn scheme_entry_builds_correct_table() {
        let entry = scheme_entry("eip155-exact", "eip155:*");
        let table = entry.as_table().unwrap();
        assert_eq!(table["id"].as_str(), Some("eip155-exact"));
        assert_eq!(table["chains"].as_str(), Some("eip155:*"));
    }

    #[test]
    fn auto_generate_schemes_creates_entries() {
        let mut doc: BTreeMap<String, toml::Value> = BTreeMap::new();
        let mut chains = toml::map::Map::new();
        chains.insert(
            "eip155:84532".into(),
            toml::Value::Table(toml::map::Map::new()),
        );
        doc.insert("chains".into(), toml::Value::Table(chains));

        auto_generate_schemes(&mut doc);

        #[cfg(feature = "chain-eip155")]
        {
            let schemes = doc["schemes"].as_array().unwrap();
            assert!(!schemes.is_empty());
            let first = schemes[0].as_table().unwrap();
            assert_eq!(first["id"].as_str(), Some("eip155-exact"));
            assert_eq!(first["chains"].as_str(), Some("eip155:*"));
        }
    }

    #[test]
    fn auto_generate_schemes_skips_when_present() {
        let mut doc: BTreeMap<String, toml::Value> = BTreeMap::new();
        let mut chains = toml::map::Map::new();
        chains.insert(
            "eip155:84532".into(),
            toml::Value::Table(toml::map::Map::new()),
        );
        doc.insert("chains".into(), toml::Value::Table(chains));

        // Pre-populate with a custom scheme
        let existing = vec![scheme_entry("custom-scheme", "eip155:1")];
        doc.insert("schemes".into(), toml::Value::Array(existing));

        auto_generate_schemes(&mut doc);

        let schemes = doc["schemes"].as_array().unwrap();
        assert_eq!(schemes.len(), 1);
        assert_eq!(
            schemes[0].as_table().unwrap()["id"].as_str(),
            Some("custom-scheme")
        );
    }

    #[test]
    fn auto_generate_schemes_fills_empty_array() {
        let mut doc: BTreeMap<String, toml::Value> = BTreeMap::new();
        let mut chains = toml::map::Map::new();
        chains.insert(
            "eip155:84532".into(),
            toml::Value::Table(toml::map::Map::new()),
        );
        doc.insert("chains".into(), toml::Value::Table(chains));
        doc.insert("schemes".into(), toml::Value::Array(vec![]));

        auto_generate_schemes(&mut doc);

        #[cfg(feature = "chain-eip155")]
        {
            let schemes = doc["schemes"].as_array().unwrap();
            assert!(!schemes.is_empty());
        }
    }

    #[test]
    fn load_config_minimal_file() {
        let config_content = "host = \"127.0.0.1\"\nport = 9090\n";
        let dir = std::env::temp_dir().join("facilitator_test_load");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("minimal.toml");
        std::fs::write(&path, config_content).unwrap();

        let config = load_config(&path).unwrap();
        assert_eq!(config.port(), 9090);
        assert_eq!(config.host(), "127.0.0.1".parse::<IpAddr>().unwrap());
        assert!(config.schemes().is_empty());

        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_dir(&dir);
    }

    #[test]
    fn load_config_nonexistent_file_errors() {
        let result = load_config(Path::new("/tmp/does_not_exist_facilitator.toml"));
        assert!(result.is_err());
    }

    #[test]
    fn load_config_invalid_toml_errors() {
        let dir = std::env::temp_dir().join("facilitator_test_invalid");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("invalid.toml");
        std::fs::write(&path, "this is [[[not valid toml").unwrap();

        let result = load_config(&path);
        assert!(result.is_err());

        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_dir(&dir);
    }
}
