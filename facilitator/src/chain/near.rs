//! NEAR chain provider construction.

use r402_core::chain::ChainId;
use r402_near::chain::NearChainReference;
use r402_near::{NearChainProvider, NearRelayer};
use serde::Deserialize;

use crate::error::AppError;

/// One relayer table: `{ account_id, secret_key }`.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct NearRelayerConfig {
    /// Relayer account (`alice.testnet` or implicit hex).
    pub account_id: String,
    /// `ed25519:…` or `secp256k1:…` secret key.
    pub secret_key: String,
}

/// Inner configuration for a NEAR chain (matches TOML structure).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct NearChainConfigInner {
    /// JSON-RPC URL. `None` uses the chain default public RPC.
    #[serde(default)]
    pub rpc: Option<String>,
    /// Relayers. Injected from `[signers].near` when absent.
    #[serde(default)]
    pub relayers: Vec<NearRelayerConfig>,
    /// Optional sponsored-gas cap (gas units). Also becomes scheme JSON.
    #[serde(default)]
    pub max_sponsored_gas: Option<u64>,
}

/// Full NEAR chain configuration with chain reference.
#[derive(Debug, Clone)]
pub(crate) struct NearChainConfig {
    /// CAIP-2 `mainnet` / `testnet`.
    pub chain_reference: NearChainReference,
    /// TOML-level configuration.
    pub inner: NearChainConfigInner,
}

impl NearChainConfig {
    /// Parse a CAIP-2 keyed chain table.
    ///
    /// # Errors
    ///
    /// Returns an error if the reference is not `mainnet`/`testnet` or the
    /// table does not match [`NearChainConfigInner`].
    pub(crate) fn from_toml(chain_id: &ChainId, value: toml::Value) -> Result<Self, AppError> {
        let chain_reference = NearChainReference::try_from(chain_id.clone())
            .map_err(|e| AppError::config_with(format!("invalid chain id '{chain_id}'"), e))?;
        let inner: NearChainConfigInner = value.try_into().map_err(|e: toml::de::Error| {
            AppError::config_with(format!("invalid [chains.\"{chain_id}\"]"), e)
        })?;
        Ok(Self {
            chain_reference,
            inner,
        })
    }

    /// Returns the CAIP-2 chain ID for this configuration.
    #[must_use]
    pub(crate) fn chain_id(&self) -> ChainId {
        self.chain_reference.into()
    }
}

/// Scheme JSON for `NearExact`. r402's config type is Deserialize-only.
#[must_use]
pub(crate) fn near_scheme_json(inner: &NearChainConfigInner) -> Option<serde_json::Value> {
    inner
        .max_sponsored_gas
        .map(|gas| serde_json::json!({ "maxSponsoredGas": gas }))
}

/// Build a NEAR provider from TOML.
///
/// # Errors
///
/// Returns an error if no relayers are configured or a secret key cannot be
/// parsed.
pub(crate) fn build_near_provider(config: &NearChainConfig) -> Result<NearChainProvider, AppError> {
    let chain_id = config.chain_id();
    if config.inner.relayers.is_empty() {
        return Err(AppError::chain(format!(
            "no relayers configured for NEAR chain {chain_id}"
        )));
    }

    let mut relayers = Vec::with_capacity(config.inner.relayers.len());
    for relayer in &config.inner.relayers {
        let built = NearRelayer::from_secret_key(&relayer.account_id, &relayer.secret_key)
            .map_err(|e| AppError::chain_with("failed to parse NEAR relayer secret key", e))?;
        relayers.push(built);
    }

    let mut provider =
        NearChainProvider::new(config.chain_reference, relayers, config.inner.rpc.clone());
    if let Some(gas) = config.inner.max_sponsored_gas {
        provider = provider.with_max_sponsored_gas(gas);
    }
    Ok(provider)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scheme_json_omitted_when_unset() {
        let inner = NearChainConfigInner {
            rpc: None,
            relayers: Vec::new(),
            max_sponsored_gas: None,
        };
        assert_eq!(near_scheme_json(&inner), None, "default is None");
    }

    #[test]
    fn scheme_json_camel_case_when_set() {
        let inner = NearChainConfigInner {
            rpc: None,
            relayers: Vec::new(),
            max_sponsored_gas: Some(42),
        };
        assert_eq!(
            near_scheme_json(&inner),
            Some(serde_json::json!({ "maxSponsoredGas": 42 })),
            "camelCase key"
        );
    }

    #[test]
    fn empty_relayers_is_startup_error() {
        let config = NearChainConfig {
            chain_reference: NearChainReference::TESTNET,
            inner: NearChainConfigInner {
                rpc: None,
                relayers: Vec::new(),
                max_sponsored_gas: None,
            },
        };
        let err = build_near_provider(&config).unwrap_err();
        assert!(
            err.to_string().contains("no relayers configured"),
            "got {err}"
        );
    }
}
