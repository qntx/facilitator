//! Hedera chain provider construction.

use r402_core::chain::ChainId;
use r402_hedera::HederaFeePayer;
use r402_hedera::chain::{HederaChainProvider, HederaChainReference};
use serde::Deserialize;

use super::{nonempty_string, reject_rpc_key};
use crate::error::AppError;

/// TOML `alias_policy` (`reject` | `allow`). Omitted → scheme JSON `None` (crate default Reject).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum HederaAliasPolicy {
    /// Reject aliases and unresolved destinations.
    Reject,
    /// Allow transfers that would create an account from an alias.
    Allow,
}

impl HederaAliasPolicy {
    /// Wire value for `HederaExactFacilitatorConfig.aliasPolicy`.
    const fn as_str(self) -> &'static str {
        match self {
            Self::Reject => "reject",
            Self::Allow => "allow",
        }
    }
}

/// One fee-payer row: Hedera account id plus a Hiero-accepted private key.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct HederaFeePayerConfig {
    /// Shard.realm.num account id (`0.0.x`).
    pub account_id: String,
    /// Private key as Hiero `PrivateKey::from_str` accepts (DER / hex / ECDSA).
    pub private_key: String,
}

/// Inner configuration for a Hedera chain (matches TOML structure).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct HederaChainConfigInner {
    /// Fee-payer accounts. Injected from `[signers].hedera` when absent.
    #[serde(default)]
    pub fee_payers: Vec<HederaFeePayerConfig>,
    /// Optional Mirror Node REST base URL.
    #[serde(default)]
    pub mirror_url: Option<String>,
    /// Optional consensus node endpoint.
    #[serde(default)]
    pub node_url: Option<String>,
    /// Optional alias policy. Set in TOML → camelCase scheme JSON; omit → crate default.
    #[serde(default)]
    pub alias_policy: Option<HederaAliasPolicy>,
}

/// Full Hedera chain configuration with chain reference.
#[derive(Debug, Clone)]
pub(crate) struct HederaChainConfig {
    /// `mainnet` or `testnet`.
    pub chain_reference: HederaChainReference,
    /// TOML-level configuration.
    pub inner: HederaChainConfigInner,
}

impl HederaChainConfig {
    /// Parse a CAIP-2 keyed chain table.
    ///
    /// # Errors
    ///
    /// Returns an error if the reference is not `mainnet`/`testnet`, `rpc` is
    /// present, or the table does not match [`HederaChainConfigInner`].
    pub(crate) fn from_toml(chain_id: &ChainId, value: toml::Value) -> Result<Self, AppError> {
        let chain_reference = HederaChainReference::try_from(chain_id.clone())
            .map_err(|e| AppError::config_with(format!("invalid chain id '{chain_id}'"), e))?;
        reject_rpc_key(
            chain_id,
            &value,
            "Hedera uses optional `mirror_url` / `node_url`",
        )?;
        let mut inner: HederaChainConfigInner =
            value.try_into().map_err(|e: toml::de::Error| {
                AppError::config_with(format!("invalid [chains.\"{chain_id}\"]"), e)
            })?;
        inner.mirror_url = nonempty_string(inner.mirror_url);
        inner.node_url = nonempty_string(inner.node_url);
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

    /// Scheme JSON: camelCase `aliasPolicy` only when TOML set it.
    ///
    /// Do not `to_value(HederaExactFacilitatorConfig)` (Deserialize-only).
    #[must_use]
    pub(crate) fn scheme_config_json(&self) -> Option<serde_json::Value> {
        self.inner
            .alias_policy
            .map(|policy| serde_json::json!({ "aliasPolicy": policy.as_str() }))
    }
}

/// Build a Hedera provider from TOML.
///
/// # Errors
///
/// Returns an error if no fee payers are configured or a secret cannot be parsed.
pub(crate) fn build_hedera_provider(
    config: &HederaChainConfig,
) -> Result<HederaChainProvider, AppError> {
    let chain_id = config.chain_id();
    if config.inner.fee_payers.is_empty() {
        return Err(AppError::chain(format!(
            "no fee_payers configured for Hedera chain {chain_id}"
        )));
    }

    let mut fee_payers = Vec::with_capacity(config.inner.fee_payers.len());
    for payer in &config.inner.fee_payers {
        if payer.account_id.is_empty() || payer.private_key.is_empty() {
            return Err(AppError::signer(
                "hedera fee_payers require account_id and private_key",
            ));
        }
        let parsed = HederaFeePayer::from_secret(&payer.account_id, &payer.private_key)
            .map_err(|e| AppError::chain_with("failed to parse Hedera fee payer", e))?;
        fee_payers.push(parsed);
    }

    let mut provider = HederaChainProvider::new(config.chain_reference, fee_payers);
    if let Some(url) = config.inner.mirror_url.as_deref() {
        provider = provider.with_mirror_url(url);
    }
    if let Some(url) = config.inner.node_url.as_deref() {
        provider = provider.with_node_url(url);
    }
    Ok(provider)
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;

    fn chain_id() -> ChainId {
        ChainId::from_str("hedera:testnet").unwrap()
    }

    #[test]
    fn from_toml_rejects_rpc() {
        let value = toml::from_str(r#"rpc = "https://example.com""#).unwrap();
        let err = HederaChainConfig::from_toml(&chain_id(), value).unwrap_err();
        assert!(err.to_string().contains("does not take `rpc`"), "got {err}");
    }

    #[test]
    fn from_toml_rejects_evm_shaped_rpc() {
        let value = toml::from_str(r#"rpc = [{ http = "https://example.com" }]"#).unwrap();
        let err = HederaChainConfig::from_toml(&chain_id(), value).unwrap_err();
        assert!(err.to_string().contains("does not take `rpc`"), "got {err}");
    }

    #[test]
    fn scheme_json_omitted_when_alias_policy_unset() {
        let value = toml::from_str(
            r#"
fee_payers = [{ account_id = "0.0.1234", private_key = "k" }]
"#,
        )
        .unwrap();
        let config = HederaChainConfig::from_toml(&chain_id(), value).unwrap();
        assert_eq!(config.scheme_config_json(), None, "default omit");
    }

    #[test]
    fn scheme_json_alias_policy_allow() {
        let value = toml::from_str(
            r#"
fee_payers = [{ account_id = "0.0.1234", private_key = "k" }]
alias_policy = "allow"
"#,
        )
        .unwrap();
        let config = HederaChainConfig::from_toml(&chain_id(), value).unwrap();
        assert_eq!(
            config.scheme_config_json(),
            Some(serde_json::json!({ "aliasPolicy": "allow" })),
            "camelCase allow"
        );
    }

    #[test]
    fn scheme_json_alias_policy_reject_when_set() {
        let value = toml::from_str(
            r#"
fee_payers = [{ account_id = "0.0.1234", private_key = "k" }]
alias_policy = "reject"
"#,
        )
        .unwrap();
        let config = HederaChainConfig::from_toml(&chain_id(), value).unwrap();
        assert_eq!(
            config.scheme_config_json(),
            Some(serde_json::json!({ "aliasPolicy": "reject" })),
            "explicit reject is still JSON"
        );
    }

    #[test]
    fn blank_mirror_and_node_urls_are_none() {
        let value = toml::from_str(
            r#"
fee_payers = [{ account_id = "0.0.1234", private_key = "k" }]
mirror_url = "   "
node_url = ""
"#,
        )
        .unwrap();
        let config = HederaChainConfig::from_toml(&chain_id(), value).unwrap();
        assert_eq!(config.inner.mirror_url, None, "blank mirror_url");
        assert_eq!(config.inner.node_url, None, "empty node_url");
    }

    #[test]
    fn build_rejects_empty_fee_payers() {
        let value = toml::Value::Table(toml::map::Map::new());
        let config = HederaChainConfig::from_toml(&chain_id(), value).unwrap();
        let err = build_hedera_provider(&config).unwrap_err();
        assert!(
            err.to_string().contains("no fee_payers configured"),
            "got {err}"
        );
    }

    #[test]
    fn build_rejects_empty_account_id() {
        let value =
            toml::from_str(r#"fee_payers = [{ account_id = "", private_key = "k" }]"#).unwrap();
        let config = HederaChainConfig::from_toml(&chain_id(), value).unwrap();
        let err = build_hedera_provider(&config).unwrap_err();
        assert!(
            err.to_string()
                .contains("require account_id and private_key"),
            "got {err}"
        );
    }

    #[test]
    fn build_rejects_empty_private_key() {
        let value =
            toml::from_str(r#"fee_payers = [{ account_id = "0.0.1234", private_key = "" }]"#)
                .unwrap();
        let config = HederaChainConfig::from_toml(&chain_id(), value).unwrap();
        let err = build_hedera_provider(&config).unwrap_err();
        assert!(
            err.to_string()
                .contains("require account_id and private_key"),
            "got {err}"
        );
    }
}
