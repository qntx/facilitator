//! Algorand chain provider construction.

use r402_algorand::AlgorandSigner;
use r402_algorand::chain::{AlgorandChainProvider, AlgorandChainReference};
use r402_core::chain::ChainId;
use serde::Deserialize;

use super::reject_rpc_key;
use crate::error::AppError;

/// Inner configuration for an Algorand chain (matches TOML structure).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AlgorandChainConfigInner {
    /// Standard-base64 32-byte seeds or 64-byte seed||pubkey. Injected from `[signers].algorand`.
    #[serde(default)]
    pub signers: Vec<String>,
    /// Optional algod REST URL (`None` → `AlgoNode` for the chain).
    #[serde(default)]
    pub algod_url: Option<String>,
    /// Optional algod API token.
    #[serde(default)]
    pub algod_token: Option<String>,
    /// Optional confirmation wait in rounds. Set in TOML → camelCase scheme JSON.
    #[serde(default)]
    pub wait_rounds: Option<u32>,
}

/// Full Algorand chain configuration with chain reference.
#[derive(Debug, Clone)]
pub(crate) struct AlgorandChainConfig {
    /// Truncated genesis CAIP-2 reference.
    pub chain_reference: AlgorandChainReference,
    /// TOML-level configuration.
    pub inner: AlgorandChainConfigInner,
}

impl AlgorandChainConfig {
    /// Parse a CAIP-2 keyed chain table.
    ///
    /// # Errors
    ///
    /// Returns an error if the reference is unknown, `rpc` is present, or the
    /// table does not match [`AlgorandChainConfigInner`].
    pub(crate) fn from_toml(chain_id: &ChainId, value: toml::Value) -> Result<Self, AppError> {
        let chain_reference = AlgorandChainReference::try_from(chain_id.clone())
            .map_err(|e| AppError::config_with(format!("invalid chain id '{chain_id}'"), e))?;
        reject_rpc_key(
            chain_id,
            &value,
            "Algorand uses optional `algod_url` / `algod_token`",
        )?;
        let inner: AlgorandChainConfigInner = value.try_into().map_err(|e: toml::de::Error| {
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

    /// Scheme JSON: camelCase `waitRounds` only when TOML set it.
    ///
    /// Do not `to_value(AlgorandExactFacilitatorConfig)` (Deserialize-only).
    #[must_use]
    pub(crate) fn scheme_config_json(&self) -> Option<serde_json::Value> {
        self.inner
            .wait_rounds
            .map(|n| serde_json::json!({ "waitRounds": n }))
    }
}

/// 12+ whitespace-separated alphabetic words: BIP39 / Algorand 25-word mnemonic, not a seed.
fn looks_like_mnemonic(secret: &str) -> bool {
    let mut count = 0usize;
    for word in secret.split_whitespace() {
        if !word.bytes().all(|b| b.is_ascii_alphabetic()) {
            return false;
        }
        count = count.saturating_add(1);
        if count >= 12 {
            return true;
        }
    }
    false
}

/// Build an Algorand provider from TOML.
///
/// # Errors
///
/// Returns an error if no signers are configured, a secret looks like a
/// mnemonic, or base64 decoding fails.
pub(crate) fn build_algorand_provider(
    config: &AlgorandChainConfig,
) -> Result<AlgorandChainProvider, AppError> {
    let chain_id = config.chain_id();
    if config.inner.signers.is_empty() {
        return Err(AppError::chain(format!(
            "no signers configured for Algorand chain {chain_id}"
        )));
    }

    let mut signers = Vec::with_capacity(config.inner.signers.len());
    for secret in &config.inner.signers {
        if looks_like_mnemonic(secret) {
            return Err(AppError::signer(
                "algorand signers must be standard-base64 32-byte seeds or 64-byte seed||pubkey; mnemonics are not supported",
            ));
        }
        let signer = AlgorandSigner::from_base64_secret(secret)
            .map_err(|e| AppError::chain_with("failed to parse Algorand signer", e))?;
        signers.push(signer);
    }

    Ok(AlgorandChainProvider::new(
        config.chain_reference,
        signers,
        config.inner.algod_url.clone(),
        config.inner.algod_token.clone(),
    ))
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use r402_core::chain::ChainProvider;

    use super::*;

    fn chain_id() -> ChainId {
        ChainId::from_str("algorand:SGO1GKSzyE7IEPItTxCByw9x8FmnrCDe").unwrap()
    }

    /// Standard-base64 of 32 zero bytes.
    const ZERO_SEED_B64: &str = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=";

    #[test]
    fn from_toml_rejects_rpc() {
        let value = toml::from_str(r#"rpc = "https://example.com""#).unwrap();
        let err = AlgorandChainConfig::from_toml(&chain_id(), value).unwrap_err();
        assert!(err.to_string().contains("does not take `rpc`"), "got {err}");
    }

    #[test]
    fn scheme_json_omitted_when_wait_rounds_unset() {
        let value = toml::from_str(r#"signers = ["AAAA"]"#).unwrap();
        let config = AlgorandChainConfig::from_toml(&chain_id(), value).unwrap();
        assert_eq!(config.scheme_config_json(), None, "default omit");
    }

    #[test]
    fn scheme_json_wait_rounds() {
        let value = toml::from_str(
            r#"
signers = ["AAAA"]
wait_rounds = 20
"#,
        )
        .unwrap();
        let config = AlgorandChainConfig::from_toml(&chain_id(), value).unwrap();
        assert_eq!(
            config.scheme_config_json(),
            Some(serde_json::json!({ "waitRounds": 20 })),
            "camelCase waitRounds"
        );
    }

    #[test]
    fn build_rejects_mnemonic() {
        let value = toml::from_str(
            r#"
signers = ["abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about"]
"#,
        )
        .unwrap();
        let config = AlgorandChainConfig::from_toml(&chain_id(), value).unwrap();
        let err = build_algorand_provider(&config).unwrap_err();
        assert!(
            err.to_string().contains("mnemonics are not supported"),
            "got {err}"
        );
    }

    #[test]
    fn build_accepts_base64_seed() {
        let value = toml::from_str(&format!("signers = [\"{ZERO_SEED_B64}\"]")).unwrap();
        let config = AlgorandChainConfig::from_toml(&chain_id(), value).unwrap();
        let provider = build_algorand_provider(&config).unwrap();
        assert_eq!(
            provider.chain_id().to_string(),
            "algorand:SGO1GKSzyE7IEPItTxCByw9x8FmnrCDe",
            "testnet"
        );
    }
}
