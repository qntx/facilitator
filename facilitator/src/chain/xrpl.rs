//! XRPL chain provider construction.

use r402_core::chain::ChainId;
use r402_xrpl::XrplChainProvider;
use r402_xrpl::chain::XrplChainReference;
use serde::Deserialize;

use crate::error::AppError;

/// Inner configuration for an XRPL chain (matches TOML structure).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct XrplChainConfigInner {
    /// JSON-RPC URL. `None` uses the default public URL for `xrpl:0/1/2`.
    #[serde(default)]
    pub rpc: Option<String>,
}

/// Full XRPL chain configuration with chain reference.
#[derive(Debug, Clone)]
pub(crate) struct XrplChainConfig {
    /// Numeric XRPL `NetworkID`.
    pub chain_reference: XrplChainReference,
    /// TOML-level configuration.
    pub inner: XrplChainConfigInner,
}

impl XrplChainConfig {
    /// Parse a CAIP-2 keyed chain table.
    ///
    /// # Errors
    ///
    /// Returns an error if the reference is not a decimal `NetworkID` or the
    /// table does not match [`XrplChainConfigInner`].
    pub(crate) fn from_toml(chain_id: &ChainId, value: toml::Value) -> Result<Self, AppError> {
        let chain_reference = XrplChainReference::try_from(chain_id.clone())
            .map_err(|e| AppError::config_with(format!("invalid chain id '{chain_id}'"), e))?;
        let inner: XrplChainConfigInner = value.try_into().map_err(|e: toml::de::Error| {
            AppError::config_with(format!("invalid [chains.\"{chain_id}\"]"), e)
        })?;
        Ok(Self {
            chain_reference,
            inner,
        })
    }
}

/// Build an XRPL provider from TOML.
///
/// # Errors
///
/// Returns an error if the chain has no default RPC and `rpc` is omitted, or
/// if `rpc` is not a valid URL.
pub(crate) fn build_xrpl_provider(config: &XrplChainConfig) -> Result<XrplChainProvider, AppError> {
    let chain_id = ChainId::from(config.chain_reference);
    XrplChainProvider::new(config.chain_reference, config.inner.rpc.clone())
        .map_err(|e| AppError::chain_with(format!("XRPL provider init failed for {chain_id}"), e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_testnet_rpc_constructs() {
        let config = XrplChainConfig {
            chain_reference: XrplChainReference::TESTNET,
            inner: XrplChainConfigInner { rpc: None },
        };
        assert!(
            build_xrpl_provider(&config).is_ok(),
            "xrpl:1 has a default URL"
        );
    }

    #[test]
    fn unknown_network_without_rpc_errors() {
        let config = XrplChainConfig {
            chain_reference: XrplChainReference::new(99),
            inner: XrplChainConfigInner { rpc: None },
        };
        let err = build_xrpl_provider(&config).unwrap_err();
        assert!(
            err.to_string()
                .contains("XRPL provider init failed for xrpl:99"),
            "got {err}"
        );
        let source = std::error::Error::source(&err).expect("source");
        assert!(
            source.to_string().contains("no default xrpl rpc url"),
            "got {source}"
        );
    }
}
