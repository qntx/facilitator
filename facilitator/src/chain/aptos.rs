//! Aptos chain provider construction.

use r402_aptos::AptosFeePayer;
use r402_aptos::chain::{AptosChainProvider, AptosChainReference};
use r402_core::chain::ChainId;
use serde::Deserialize;

use super::require_string_rpc;
use crate::error::AppError;

/// Inner configuration for an Aptos chain (matches TOML structure).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AptosChainConfigInner {
    /// 32-byte ed25519 private keys as hex. Injected from `[signers].aptos`.
    #[serde(default)]
    pub fee_payers: Vec<String>,
    /// Optional fullnode REST URL (`None` → network default).
    #[serde(default)]
    pub rpc: Option<String>,
    /// Optional sponsorship flag. Set in TOML → provider + camelCase scheme JSON.
    #[serde(default)]
    pub sponsor_transactions: Option<bool>,
}

/// Full Aptos chain configuration with chain reference.
#[derive(Debug, Clone)]
pub(crate) struct AptosChainConfig {
    /// `1` (mainnet) or `2` (testnet).
    pub chain_reference: AptosChainReference,
    /// TOML-level configuration.
    pub inner: AptosChainConfigInner,
}

impl AptosChainConfig {
    /// Parse a CAIP-2 keyed chain table.
    ///
    /// # Errors
    ///
    /// Returns an error if the reference is not `1`/`2`, `rpc` is present but
    /// not a string, or the table does not match [`AptosChainConfigInner`].
    pub(crate) fn from_toml(chain_id: &ChainId, value: toml::Value) -> Result<Self, AppError> {
        let chain_reference = AptosChainReference::try_from(chain_id.clone())
            .map_err(|e| AppError::config_with(format!("invalid chain id '{chain_id}'"), e))?;
        require_string_rpc(chain_id, &value)?;
        let inner: AptosChainConfigInner = value.try_into().map_err(|e: toml::de::Error| {
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

    /// Scheme JSON: camelCase `sponsorTransactions` only when TOML set it.
    ///
    /// Do not `to_value(AptosExactFacilitatorConfig)` (Deserialize-only).
    #[must_use]
    pub(crate) fn scheme_config_json(&self) -> Option<serde_json::Value> {
        self.inner
            .sponsor_transactions
            .map(|sponsor| serde_json::json!({ "sponsorTransactions": sponsor }))
    }
}

/// Build an Aptos provider from TOML.
///
/// # Errors
///
/// Returns an error if no fee payers are configured, a key cannot be parsed,
/// or the SDK client cannot be constructed.
pub(crate) fn build_aptos_provider(
    config: &AptosChainConfig,
) -> Result<AptosChainProvider, AppError> {
    let chain_id = config.chain_id();
    if config.inner.fee_payers.is_empty() {
        return Err(AppError::chain(format!(
            "no fee_payers configured for Aptos chain {chain_id}"
        )));
    }

    let mut fee_payers = Vec::with_capacity(config.inner.fee_payers.len());
    for key in &config.inner.fee_payers {
        let parsed = AptosFeePayer::from_private_key_hex(key)
            .map_err(|e| AppError::chain_with("failed to parse Aptos fee payer", e))?;
        fee_payers.push(parsed);
    }

    let mut provider = AptosChainProvider::new(
        config.chain_reference,
        fee_payers,
        config.inner.rpc.as_deref(),
    )
    .map_err(|e| AppError::chain(format!("Aptos provider init failed: {e}")))?;
    if let Some(sponsor) = config.inner.sponsor_transactions {
        provider = provider.with_sponsor_transactions(sponsor);
    }
    Ok(provider)
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use r402_core::chain::ChainProvider;

    use super::*;

    fn chain_id() -> ChainId {
        ChainId::from_str("aptos:2").unwrap()
    }

    #[test]
    fn from_toml_rejects_evm_shaped_rpc() {
        let value = toml::from_str(r#"rpc = [{ http = "https://example.com" }]"#).unwrap();
        let err = AptosChainConfig::from_toml(&chain_id(), value).unwrap_err();
        assert!(
            err.to_string().contains("`rpc` must be a string URL"),
            "got {err}"
        );
    }

    #[test]
    fn scheme_json_omitted_when_sponsor_unset() {
        let value = toml::from_str(r#"fee_payers = ["00"]"#).unwrap();
        let config = AptosChainConfig::from_toml(&chain_id(), value).unwrap();
        assert_eq!(config.scheme_config_json(), None, "default omit");
        assert_eq!(config.inner.rpc, None, "rpc optional");
    }

    #[test]
    fn scheme_json_sponsor_transactions() {
        let value = toml::from_str(
            r#"
fee_payers = ["00"]
sponsor_transactions = false
rpc = "https://fullnode.testnet.aptoslabs.com/v1"
"#,
        )
        .unwrap();
        let config = AptosChainConfig::from_toml(&chain_id(), value).unwrap();
        assert_eq!(
            config.scheme_config_json(),
            Some(serde_json::json!({ "sponsorTransactions": false })),
            "camelCase sponsorTransactions"
        );
        assert_eq!(
            config.inner.rpc.as_deref(),
            Some("https://fullnode.testnet.aptoslabs.com/v1"),
            "string rpc"
        );
    }

    #[test]
    fn build_accepts_ed25519_hex() {
        let value = toml::from_str(
            r#"fee_payers = ["0000000000000000000000000000000000000000000000000000000000000001"]"#,
        )
        .unwrap();
        let config = AptosChainConfig::from_toml(&chain_id(), value).unwrap();
        let provider = build_aptos_provider(&config).unwrap();
        assert_eq!(provider.chain_id().to_string(), "aptos:2", "testnet");
    }
}
