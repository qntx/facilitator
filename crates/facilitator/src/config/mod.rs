//! TOML config schema for facilitator 2.0.

mod family;
mod http;
mod literal;
mod network;
mod scheme;

use std::collections::BTreeMap;
use std::io::Write;
use std::path::Path;
use std::str::FromStr;

use family::{FamilyStatus, classify};
pub use http::{HttpAuth, HttpConfig, LogConfig, LogFormat};
use literal::{reject_literals_and_obsolete, reject_obsolete_root};
use network::resolve_rpc;
pub use network::{EvmNetwork, Network, RpcConfig, RpcEndpoint, SvmNetwork};
use r402_protocol::ChainId;
pub use scheme::{
    BuilderCodeToml, EvmSchemeConfig, SchemeTables, SvmExactConfig, SvmSchemeConfig, SvmUptoConfig,
};
use serde::Deserialize;

use crate::error::Error;
use crate::secrets::SecretSource;

/// Fully parsed configuration (secrets not yet substituted).
#[derive(Debug, Clone)]
pub struct Config {
    /// HTTP process settings.
    pub http: HttpConfig,
    /// Log settings.
    pub log: LogConfig,
    /// Named signers.
    pub signers: BTreeMap<String, SecretSource>,
    /// Global scheme knobs.
    pub scheme: SchemeTables,
    /// Networks in TOML key order.
    pub networks: Vec<Network>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawConfig {
    /// HTTP table.
    #[serde(default)]
    http: HttpConfig,
    /// Log table.
    #[serde(default)]
    log: LogConfig,
    /// Named `[signer.*]` tables.
    #[serde(default)]
    signer: BTreeMap<String, SecretSource>,
    /// `[scheme.*]` tables.
    #[serde(default)]
    scheme: SchemeTables,
    /// `[network."<caip2>"]` tables.
    #[serde(default)]
    network: BTreeMap<String, toml::Value>,
    /// Reserved discovery table.
    #[serde(default)]
    discovery: Option<DiscoveryConfig>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct DiscoveryConfig {
    /// Must stay false until discovery is implemented.
    enabled: bool,
}

/// Load TOML from `path` and apply env overlays.
///
/// # Errors
///
/// Read, parse, validation, or overlay failures.
pub fn load_config(path: &Path) -> Result<Config, Error> {
    let raw = std::fs::read_to_string(path)
        .map_err(|err| Error::config_with(format!("failed to read '{}'", path.display()), err))?;
    let mut config = parse_config_toml(&raw)?;
    config.overlay_env()?;
    Ok(config)
}

/// Parse a TOML document. Does not overlay env or resolve secrets.
///
/// # Errors
///
/// Unknown fields, obsolete keys, literal secrets, invalid families/schemes.
pub fn parse_config_toml(raw: &str) -> Result<Config, Error> {
    let doc: toml::Value = toml::from_str(raw)
        .map_err(|err| Error::config_with("failed to parse config TOML", err))?;
    let table = doc
        .as_table()
        .ok_or_else(|| Error::config("config root must be a table"))?;
    reject_obsolete_root(table)?;
    reject_literals_and_obsolete(&doc)?;
    let parsed: RawConfig = doc
        .try_into()
        .map_err(|err: toml::de::Error| Error::config_with("invalid config", err))?;
    reject_discovery(parsed.discovery.as_ref())?;
    let networks = parse_networks(parsed.network)?;
    let config = Config {
        http: parsed.http,
        log: parsed.log,
        signers: parsed.signer,
        scheme: parsed.scheme,
        networks,
    };
    config.validate()?;
    Ok(config)
}

fn reject_discovery(discovery: Option<&DiscoveryConfig>) -> Result<(), Error> {
    if discovery.is_some_and(|d| d.enabled) {
        return Err(Error::config(
            "discovery.enabled = true is not implemented in this build",
        ));
    }
    Ok(())
}

fn parse_networks(raw: BTreeMap<String, toml::Value>) -> Result<Vec<Network>, Error> {
    if raw.is_empty() {
        return Err(Error::config(
            "empty [network]; a facilitator with no kinds is useless",
        ));
    }
    let mut networks = Vec::with_capacity(raw.len());
    for (key, value) in raw {
        let chain_id = ChainId::from_str(&key)
            .map_err(|err| Error::config_with(format!("invalid chain id '{key}'"), err))?;
        match classify(chain_id.namespace()) {
            FamilyStatus::Hostable(family) => {
                networks.push(network::parse_network(&chain_id, family, value)?);
            }
            FamilyStatus::CompiledOut { feature } => {
                return Err(Error::config(format!(
                    "compiled-out family '{}' in [network.\"{key}\"]; rebuild with --features {feature}",
                    chain_id.namespace()
                )));
            }
            FamilyStatus::Reserved { feature: _ } => {
                return Err(Error::config(format!(
                    "family '{}' schema reserved; not constructed in this build",
                    chain_id.namespace()
                )));
            }
            FamilyStatus::Unknown => {
                return Err(Error::config(format!(
                    "unknown CAIP-2 namespace '{}' in [network.\"{key}\"]",
                    chain_id.namespace()
                )));
            }
        }
    }
    Ok(networks)
}

impl Config {
    /// Overlay `FACILITATOR_HTTP_*`, `FACILITATOR_LOG_LEVEL`, and `RUST_LOG`.
    ///
    /// # Errors
    ///
    /// Invalid socket addresses in overlay env vars.
    pub fn overlay_env(&mut self) -> Result<(), Error> {
        if let Ok(raw) = std::env::var("FACILITATOR_HTTP_LISTEN") {
            self.http.listen = raw.parse().map_err(|err| {
                Error::config_with(format!("invalid FACILITATOR_HTTP_LISTEN '{raw}'"), err)
            })?;
        }
        if let Ok(raw) = std::env::var("FACILITATOR_HTTP_METRICS_LISTEN") {
            self.http.metrics_listen = Some(raw.parse().map_err(|err| {
                Error::config_with(
                    format!("invalid FACILITATOR_HTTP_METRICS_LISTEN '{raw}'"),
                    err,
                )
            })?);
        }
        if let Ok(level) = std::env::var("FACILITATOR_LOG_LEVEL") {
            self.log.level = level;
        }
        if let Ok(level) = std::env::var("RUST_LOG") {
            self.log.level = level;
        }
        Ok(())
    }

    /// Resolve every signer and `rpc_env`. Does not keep secret material.
    ///
    /// # Errors
    ///
    /// Missing env, unreadable file, or invalid RPC URL from env.
    pub fn resolve_secrets(&self, lookup: &impl Fn(&str) -> Option<String>) -> Result<(), Error> {
        for (name, signer) in &self.signers {
            signer.resolve(lookup).map_err(|err| match err {
                Error::Secret { context, source } => Error::Secret {
                    context: format!("signer '{name}': {context}"),
                    source,
                },
                other => other,
            })?;
        }
        for network in &self.networks {
            let _ = resolve_rpc(network.chain_id(), rpc_of(network), lookup)?;
        }
        Ok(())
    }

    /// Print parsed networks and schemes.
    ///
    /// # Errors
    ///
    /// Write failures.
    pub fn write_summary(&self, mut out: impl Write) -> std::io::Result<()> {
        for network in &self.networks {
            let schemes = network.schemes().join(",");
            let signers = network.signer_names().join(",");
            writeln!(
                out,
                "{} schemes=[{schemes}] signers=[{signers}]",
                network.chain_id()
            )?;
        }
        Ok(())
    }

    fn validate(&self) -> Result<(), Error> {
        self.validate_signer_refs()?;
        self.validate_timeouts()?;
        Ok(())
    }

    fn validate_signer_refs(&self) -> Result<(), Error> {
        for network in &self.networks {
            self.require_known_signers(network)?;
        }
        Ok(())
    }

    /// Each network must reference named `[signer.*]` entries.
    fn require_known_signers(&self, network: &Network) -> Result<(), Error> {
        for name in network.signer_names() {
            self.require_signer(network.chain_id(), name)?;
        }
        Ok(())
    }

    /// Fail if `name` is not a configured signer.
    fn require_signer(&self, chain_id: &ChainId, name: &str) -> Result<(), Error> {
        if self.signers.contains_key(name) {
            return Ok(());
        }
        Err(Error::config(format!(
            "[network.\"{chain_id}\"] references unknown signer '{name}'"
        )))
    }

    fn validate_timeouts(&self) -> Result<(), Error> {
        let settle = self.http.settle_timeout.as_secs();
        for network in &self.networks {
            let Network::Evm(evm) = network else {
                continue;
            };
            if evm.receipt_timeout_secs.saturating_add(2) > settle {
                return Err(Error::config(format!(
                    "[network.\"{}\"] receipt_timeout_secs ({}) + 2 must be <= settle_timeout ({settle}s)",
                    evm.chain_id, evm.receipt_timeout_secs
                )));
            }
        }
        Ok(())
    }
}

const fn rpc_of(network: &Network) -> &RpcConfig {
    match network {
        Network::Evm(net) => &net.rpc,
        Network::Svm(net) => &net.rpc,
    }
}
