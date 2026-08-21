//! Stellar chain provider construction.

use r402_core::chain::ChainId;
use r402_stellar::chain::StellarChainReference;
use r402_stellar::{StellarChainProvider, StellarSigner};
use serde::Deserialize;

use super::{nonempty_string, require_string_url};
use crate::error::AppError;

/// Inner configuration for a Stellar chain (matches TOML structure).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct StellarChainConfigInner {
    /// `S…` secret seeds. Injected from `[signers].stellar`.
    #[serde(default)]
    pub signers: Vec<String>,
    /// Optional Soroban RPC URL. Pubnet requires a non-empty value.
    #[serde(default)]
    pub rpc: Option<String>,
    /// Optional Horizon base URL override.
    #[serde(default)]
    pub horizon_url: Option<String>,
    /// Optional fee-bump `S…` secret. Injected from `[signers].stellar_fee_bump`.
    #[serde(default)]
    pub fee_bump: Option<String>,
    /// Optional settlement-fee safety ceiling (stroops).
    #[serde(default)]
    pub max_transaction_fee_stroops: Option<u32>,
}

/// Full Stellar chain configuration with chain reference.
#[derive(Debug, Clone)]
pub(crate) struct StellarChainConfig {
    /// `pubnet` or `testnet`.
    pub chain_reference: StellarChainReference,
    /// TOML-level configuration.
    pub inner: StellarChainConfigInner,
}

impl StellarChainConfig {
    /// Parse a CAIP-2 keyed chain table.
    ///
    /// # Errors
    ///
    /// Returns an error if the reference is not `pubnet`/`testnet`, `rpc` is
    /// present but not a string, or the table does not match
    /// [`StellarChainConfigInner`].
    pub(crate) fn from_toml(chain_id: &ChainId, value: toml::Value) -> Result<Self, AppError> {
        let chain_reference = StellarChainReference::try_from(chain_id.clone())
            .map_err(|e| AppError::config_with(format!("invalid chain id '{chain_id}'"), e))?;
        require_string_url(chain_id, &value, "rpc")?;
        let mut inner: StellarChainConfigInner =
            value.try_into().map_err(|e: toml::de::Error| {
                AppError::config_with(format!("invalid [chains.\"{chain_id}\"]"), e)
            })?;
        inner.rpc = nonempty_string(inner.rpc);
        inner.horizon_url = nonempty_string(inner.horizon_url);
        inner.fee_bump = nonempty_string(inner.fee_bump);
        inner.signers = inner
            .signers
            .into_iter()
            .filter_map(|s| nonempty_string(Some(s)))
            .collect();
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

/// Build a Stellar provider from TOML.
///
/// # Errors
///
/// Returns an error if no signers are configured, a secret cannot be parsed,
/// pubnet is missing `rpc`, or the RPC client cannot be constructed.
pub(crate) fn build_stellar_provider(
    config: &StellarChainConfig,
) -> Result<StellarChainProvider, AppError> {
    let chain_id = config.chain_id();
    if config.inner.signers.is_empty() {
        return Err(AppError::chain(format!(
            "no signers configured for Stellar chain {chain_id}"
        )));
    }

    let mut signers = Vec::with_capacity(config.inner.signers.len());
    for secret in &config.inner.signers {
        let signer = StellarSigner::from_secret(secret)
            .map_err(|e| AppError::chain_with("failed to parse Stellar signer", e))?;
        signers.push(signer);
    }

    let mut provider =
        StellarChainProvider::new(config.chain_reference, signers, config.inner.rpc.as_deref())
            .map_err(|e| AppError::chain_with("Stellar provider init failed", e))?;
    if let Some(url) = config.inner.horizon_url.as_deref() {
        provider = provider.with_horizon_url(url);
    }
    if let Some(fee) = config.inner.max_transaction_fee_stroops {
        provider = provider.with_max_transaction_fee_stroops(fee);
    }
    if let Some(secret) = config.inner.fee_bump.as_deref() {
        let bump = StellarSigner::from_secret(secret)
            .map_err(|e| AppError::chain_with("failed to parse Stellar fee-bump signer", e))?;
        provider = provider.with_fee_bump_signer(bump);
    }
    Ok(provider)
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use r402_core::chain::ChainProvider;

    use super::*;

    fn testnet_id() -> ChainId {
        ChainId::from_str("stellar:testnet").unwrap()
    }

    fn pubnet_id() -> ChainId {
        ChainId::from_str("stellar:pubnet").unwrap()
    }

    /// Valid `S…` seed used in r402-stellar tests.
    const SECRET: &str = "SCKB3ECHCPVM4HJPNCQWTQWJJ5XRL6UNKLTTCIH4B7TB22NKJ5GUFMIV";

    #[test]
    fn from_toml_rejects_evm_shaped_rpc() {
        let value = toml::from_str(r#"rpc = [{ http = "https://example.com" }]"#).unwrap();
        let err = StellarChainConfig::from_toml(&testnet_id(), value).unwrap_err();
        assert!(
            err.to_string().contains("`rpc` must be a string URL"),
            "got {err}"
        );
    }

    #[test]
    fn from_toml_rpc_optional_on_testnet() {
        let value = toml::from_str(&format!("signers = [\"{SECRET}\"]")).unwrap();
        let config = StellarChainConfig::from_toml(&testnet_id(), value).unwrap();
        assert_eq!(config.inner.rpc, None, "rpc optional");
        assert_eq!(config.inner.signers.len(), 1, "one signer");
    }

    #[test]
    fn build_rejects_empty_signers() {
        let value = toml::from_str("signers = []").unwrap();
        let config = StellarChainConfig::from_toml(&testnet_id(), value).unwrap();
        let err = build_stellar_provider(&config).unwrap_err();
        assert!(
            err.to_string().contains("no signers configured"),
            "got {err}"
        );
    }

    #[test]
    fn from_toml_trims_secrets_and_blank_urls() {
        let value = toml::from_str(&format!(
            "signers = [\" {SECRET} \\n\", \"  \"]\nfee_bump = \" {SECRET} \"\nhorizon_url = \"  \"\nrpc = \"  \""
        ))
        .unwrap();
        let config = StellarChainConfig::from_toml(&testnet_id(), value).unwrap();
        assert_eq!(
            config.inner.signers.as_slice(),
            &[SECRET.to_owned()],
            "trim"
        );
        assert_eq!(
            config.inner.fee_bump.as_deref(),
            Some(SECRET),
            "fee_bump trim"
        );
        assert_eq!(config.inner.horizon_url, None, "blank horizon");
        assert_eq!(config.inner.rpc, None, "blank rpc");
    }

    #[test]
    fn build_rejects_whitespace_only_signers() {
        let value = toml::from_str("signers = [\"  \"]").unwrap();
        let config = StellarChainConfig::from_toml(&testnet_id(), value).unwrap();
        let err = build_stellar_provider(&config).unwrap_err();
        assert!(
            err.to_string().contains("no signers configured"),
            "got {err}"
        );
    }

    #[test]
    fn build_testnet_without_rpc() {
        let value = toml::from_str(&format!("signers = [\"{SECRET}\"]")).unwrap();
        let config = StellarChainConfig::from_toml(&testnet_id(), value).unwrap();
        let provider = build_stellar_provider(&config).unwrap();
        assert_eq!(
            provider.chain_id().to_string(),
            "stellar:testnet",
            "testnet"
        );
    }

    #[test]
    fn build_pubnet_requires_rpc() {
        let value = toml::from_str(&format!("signers = [\"{SECRET}\"]")).unwrap();
        let config = StellarChainConfig::from_toml(&pubnet_id(), value).unwrap();
        let err = build_stellar_provider(&config).unwrap_err();
        assert!(
            err.to_string().contains("Stellar provider init failed"),
            "got {err}"
        );
    }

    #[test]
    fn build_pubnet_with_rpc() {
        let value = toml::from_str(&format!(
            "signers = [\"{SECRET}\"]\nrpc = \"https://soroban.example\""
        ))
        .unwrap();
        let config = StellarChainConfig::from_toml(&pubnet_id(), value).unwrap();
        let provider = build_stellar_provider(&config).unwrap();
        assert_eq!(provider.chain_id().to_string(), "stellar:pubnet", "pubnet");
    }
}
