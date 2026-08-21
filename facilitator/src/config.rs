//! Configuration loading.

use std::collections::BTreeMap;
use std::net::IpAddr;
use std::path::Path;
use std::str::FromStr;

use r402_core::chain::ChainId;
use serde::Deserialize;

#[cfg(feature = "chain-algorand")]
use crate::chain::algorand::AlgorandChainConfig;
#[cfg(feature = "chain-aptos")]
use crate::chain::aptos::AptosChainConfig;
#[cfg(feature = "chain-eip155")]
use crate::chain::eip155::Eip155ChainConfig;
use crate::chain::family_feature;
#[cfg(feature = "chain-hedera")]
use crate::chain::hedera::HederaChainConfig;
use crate::error::AppError;
use crate::signers;

/// Server configuration combining host/port and chain configs.
#[derive(Debug, Clone)]
pub(crate) struct Config {
    /// Bind address (default: 0.0.0.0).
    host: IpAddr,
    /// Listen port (default: 8080).
    port: u16,
    /// Log level filter (default: "info").
    log_level: String,
    /// Parsed chain configurations.
    chains: ChainsConfig,
}

/// Ordered collection of per-family chain configs.
#[derive(Debug, Clone, Default)]
pub(crate) struct ChainsConfig {
    /// EIP-155 chains, in TOML key order.
    #[cfg(feature = "chain-eip155")]
    eip155: Vec<Eip155ChainConfig>,
    /// Hedera chains, in TOML key order.
    #[cfg(feature = "chain-hedera")]
    hedera: Vec<HederaChainConfig>,
    /// Algorand chains, in TOML key order.
    #[cfg(feature = "chain-algorand")]
    algorand: Vec<AlgorandChainConfig>,
    /// Aptos chains, in TOML key order.
    #[cfg(feature = "chain-aptos")]
    aptos: Vec<AptosChainConfig>,
}

impl ChainsConfig {
    /// EIP-155 chain configs.
    #[cfg(feature = "chain-eip155")]
    #[must_use]
    pub(crate) fn eip155(&self) -> &[Eip155ChainConfig] {
        &self.eip155
    }

    /// Hedera chain configs.
    #[cfg(feature = "chain-hedera")]
    #[must_use]
    pub(crate) fn hedera(&self) -> &[HederaChainConfig] {
        &self.hedera
    }

    /// Algorand chain configs.
    #[cfg(feature = "chain-algorand")]
    #[must_use]
    pub(crate) fn algorand(&self) -> &[AlgorandChainConfig] {
        &self.algorand
    }

    /// Aptos chain configs.
    #[cfg(feature = "chain-aptos")]
    #[must_use]
    pub(crate) fn aptos(&self) -> &[AptosChainConfig] {
        &self.aptos
    }
}

/// Wire shape of the TOML file after signer injection.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawConfig {
    /// Bind address (default: 0.0.0.0).
    #[serde(default = "default_host")]
    host: IpAddr,
    /// Listen port (default: 8080).
    #[serde(default = "default_port")]
    port: u16,
    /// Log level filter (default: "info").
    #[serde(default = "default_log_level")]
    log_level: String,
    /// Raw chain tables keyed by CAIP-2 identifier.
    #[serde(default)]
    chains: BTreeMap<String, toml::Value>,
}

/// Default bind address: `HOST` env, else all interfaces.
fn default_host() -> IpAddr {
    std::env::var("HOST")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED))
}

/// Default listen port, overridable via `PORT` env var.
fn default_port() -> u16 {
    std::env::var("PORT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(8080)
}

/// Default log level filter string.
fn default_log_level() -> String {
    "info".to_owned()
}

impl Config {
    /// Returns the configured bind address.
    #[must_use]
    pub(crate) const fn host(&self) -> IpAddr {
        self.host
    }

    /// Returns the configured listen port.
    #[must_use]
    pub(crate) const fn port(&self) -> u16 {
        self.port
    }

    /// Returns the configured log level filter string.
    #[must_use]
    pub(crate) fn log_level(&self) -> &str {
        &self.log_level
    }

    /// Returns a reference to the chain configurations.
    #[must_use]
    pub(crate) const fn chains(&self) -> &ChainsConfig {
        &self.chains
    }
}

/// Load configuration from a TOML file at the given path.
///
/// # Errors
///
/// Returns an error if the file cannot be resolved, read, or parsed, if
/// `[[schemes]]` is present, if `[chains]` is empty, or if a chain family is
/// unknown / compiled out.
pub(crate) fn load_config(path: &Path) -> Result<Config, AppError> {
    let config_path = path
        .canonicalize()
        .map_err(|e| AppError::config_with(format!("failed to resolve '{}'", path.display()), e))?;
    let raw_content = std::fs::read_to_string(&config_path).map_err(|e| {
        AppError::config_with(format!("failed to read '{}'", config_path.display()), e)
    })?;
    parse_config_toml(&raw_content)
}

/// Parse a TOML config document.
///
/// # Errors
///
/// Same as [`load_config`].
pub(crate) fn parse_config_toml(raw: &str) -> Result<Config, AppError> {
    let mut doc: BTreeMap<String, toml::Value> =
        toml::from_str(raw).map_err(|e| AppError::config_with("failed to parse config TOML", e))?;
    if doc.contains_key("schemes") {
        return Err(AppError::config(
            "delete [[schemes]]; schemes come from Cargo features",
        ));
    }
    signers::preprocess_signers(&mut doc)?;
    let table: toml::map::Map<String, toml::Value> = doc.into_iter().collect();
    let parsed: RawConfig = toml::Value::Table(table)
        .try_into()
        .map_err(|e: toml::de::Error| AppError::config_with("invalid config", e))?;
    let chains = parse_chains(parsed.chains)?;
    Ok(Config {
        host: parsed.host,
        port: parsed.port,
        log_level: parsed.log_level,
        chains,
    })
}

/// Parse CAIP-2 keyed chain tables into compiled family configs.
fn parse_chains(raw: BTreeMap<String, toml::Value>) -> Result<ChainsConfig, AppError> {
    if raw.is_empty() {
        return Err(AppError::config(
            "empty [chains]; a facilitator with no kinds is useless",
        ));
    }

    #[cfg(feature = "chain-eip155")]
    let mut eip155 = Vec::new();
    #[cfg(feature = "chain-hedera")]
    let mut hedera = Vec::new();
    #[cfg(feature = "chain-algorand")]
    let mut algorand = Vec::new();
    #[cfg(feature = "chain-aptos")]
    let mut aptos = Vec::new();

    for (key, value) in raw {
        let chain_id = ChainId::from_str(&key)
            .map_err(|e| AppError::config_with(format!("invalid chain id '{key}'"), e))?;
        match chain_id.namespace() {
            #[cfg(feature = "chain-eip155")]
            "eip155" => eip155.push(Eip155ChainConfig::from_toml(&chain_id, value)?),
            #[cfg(feature = "chain-hedera")]
            "hedera" => hedera.push(HederaChainConfig::from_toml(&chain_id, value)?),
            #[cfg(feature = "chain-algorand")]
            "algorand" => algorand.push(AlgorandChainConfig::from_toml(&chain_id, value)?),
            #[cfg(feature = "chain-aptos")]
            "aptos" => aptos.push(AptosChainConfig::from_toml(&chain_id, value)?),
            other => {
                if let Some(feature) = family_feature(other) {
                    return Err(AppError::config(format!(
                        "compiled-out family '{other}' in [chains.\"{key}\"]; rebuild with --features {feature}"
                    )));
                }
                return Err(AppError::config(format!(
                    "unknown CAIP-2 namespace '{other}' in [chains.\"{key}\"]"
                )));
            }
        }
    }

    Ok(ChainsConfig {
        #[cfg(feature = "chain-eip155")]
        eip155,
        #[cfg(feature = "chain-hedera")]
        hedera,
        #[cfg(feature = "chain-algorand")]
        algorand,
        #[cfg(feature = "chain-aptos")]
        aptos,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_rejects_schemes_table() {
        let raw = r#"
host = "127.0.0.1"
[[schemes]]
id = "eip155-exact"
[chains."eip155:84532"]
rpc = [{ http = "https://example.com" }]
signers = ["0xabc"]
"#;
        let err = parse_config_toml(raw).unwrap_err();
        assert!(err.to_string().contains("delete [[schemes]]"), "got {err}");
    }

    #[test]
    fn parse_rejects_empty_chains() {
        let raw = "host = \"127.0.0.1\"\nport = 8080\n";
        let err = parse_config_toml(raw).unwrap_err();
        assert!(err.to_string().contains("empty [chains]"), "got {err}");
    }

    #[test]
    fn parse_rejects_unknown_namespace() {
        let raw = r#"
[chains."foo:bar"]
rpc = "https://example.com"
"#;
        let err = parse_config_toml(raw).unwrap_err();
        assert!(
            err.to_string().contains("unknown CAIP-2 namespace"),
            "got {err}"
        );
    }

    #[test]
    fn parse_rejects_compiled_out_solana() {
        let raw = r#"
[chains."solana:EtWTRABZaYq6iMfeYKouRu166VU2xqa1"]
rpc = "https://api.devnet.solana.com"
"#;
        let err = parse_config_toml(raw).unwrap_err();
        assert!(
            err.to_string().contains("compiled-out family 'solana'"),
            "got {err}"
        );
    }

    #[cfg(feature = "chain-eip155")]
    #[test]
    fn parse_eip155_minimal() {
        let raw = r#"
host = "127.0.0.1"
port = 9090
[signers]
evm = ["0xabc"]
[chains."eip155:84532"]
rpc = [{ http = "https://sepolia.base.org" }]
"#;
        let config = parse_config_toml(raw).unwrap();
        assert_eq!(config.port(), 9090, "port");
        let chain = config.chains().eip155().first().expect("one eip155 chain");
        assert_eq!(
            chain.inner.receipt_timeout_secs, 20,
            "default receipt timeout"
        );
        assert_eq!(
            chain.inner.signers.as_slice(),
            &["0xabc".to_owned()],
            "injected signer"
        );
    }

    #[cfg(not(feature = "chain-hedera"))]
    #[test]
    fn parse_rejects_compiled_out_hedera() {
        let raw = r#"
[chains."hedera:testnet"]
"#;
        let err = parse_config_toml(raw).unwrap_err();
        assert!(
            err.to_string().contains("compiled-out family 'hedera'"),
            "got {err}"
        );
    }

    #[cfg(feature = "chain-hedera")]
    #[test]
    fn parse_hedera_minimal() {
        let raw = r#"
[signers]
hedera = [{ account_id = "0.0.1234", private_key = "secret" }]
[chains."hedera:testnet"]
alias_policy = "allow"
"#;
        let config = parse_config_toml(raw).unwrap();
        let chain = config.chains().hedera().first().expect("one hedera chain");
        assert_eq!(
            chain
                .inner
                .fee_payers
                .first()
                .map(|p| p.account_id.as_str()),
            Some("0.0.1234"),
            "injected fee payer"
        );
        assert_eq!(
            chain.scheme_config_json(),
            Some(serde_json::json!({ "aliasPolicy": "allow" })),
            "scheme JSON"
        );
    }

    #[cfg(feature = "chain-algorand")]
    #[test]
    fn parse_algorand_minimal() {
        let raw = r#"
[signers]
algorand = ["AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA="]
[chains."algorand:SGO1GKSzyE7IEPItTxCByw9x8FmnrCDe"]
wait_rounds = 8
"#;
        let config = parse_config_toml(raw).unwrap();
        let chain = config
            .chains()
            .algorand()
            .first()
            .expect("one algorand chain");
        assert_eq!(chain.inner.signers.len(), 1, "injected signer");
        assert_eq!(
            chain.scheme_config_json(),
            Some(serde_json::json!({ "waitRounds": 8 })),
            "scheme JSON"
        );
    }

    #[cfg(feature = "chain-aptos")]
    #[test]
    fn parse_aptos_minimal() {
        let raw = r#"
[signers]
aptos = ["0000000000000000000000000000000000000000000000000000000000000001"]
[chains."aptos:2"]
sponsor_transactions = false
"#;
        let config = parse_config_toml(raw).unwrap();
        let chain = config.chains().aptos().first().expect("one aptos chain");
        assert_eq!(chain.inner.fee_payers.len(), 1, "injected fee payer");
        assert_eq!(
            chain.scheme_config_json(),
            Some(serde_json::json!({ "sponsorTransactions": false })),
            "scheme JSON"
        );
    }

    #[test]
    fn load_config_nonexistent_file_errors() {
        let result = load_config(Path::new("/tmp/does_not_exist_facilitator.toml"));
        assert!(result.is_err(), "missing file");
    }

    #[test]
    fn load_config_invalid_toml_errors() {
        let dir = std::env::temp_dir().join("facilitator_test_invalid");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("invalid.toml");
        std::fs::write(&path, "this is [[[not valid toml").unwrap();
        assert!(load_config(&path).is_err(), "invalid toml");
        drop(std::fs::remove_file(&path));
        drop(std::fs::remove_dir(&dir));
    }
}
