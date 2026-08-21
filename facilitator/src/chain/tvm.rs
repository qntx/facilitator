//! TON (TVM) chain provider construction.

use r402_core::chain::ChainId;
use r402_tvm::chain::{TvmChainReference, TvmProviderKind};
use r402_tvm::{HighloadV3Config, TvmChainProvider};
use serde::Deserialize;

use super::require_string_rpc;
use crate::error::AppError;

/// Inner configuration for a TVM chain (matches TOML structure).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TvmChainConfigInner {
    /// Hex or base64 32- or 64-byte key. Injected from `[signers].tvm`.
    #[serde(default)]
    pub signer: String,
    /// Optional REST API key.
    #[serde(default)]
    pub api_key: Option<String>,
    /// Highload V3 subwallet id (crate default `0x10ad`).
    #[serde(default)]
    pub subwallet_id: Option<u32>,
    /// Highload V3 timeout seconds (crate default `3600`).
    #[serde(default)]
    pub highload_timeout: Option<u32>,
    /// Nanotons attached per relayed inner message (crate default `40_000_000`).
    #[serde(default)]
    pub relay_amount: Option<u64>,
    /// REST provider: `toncenter` | `tonapi` (crate default `toncenter`).
    #[serde(default)]
    pub provider: Option<String>,
    /// Optional REST base URL (`rpc` alias for `provider_base_url`).
    #[serde(default)]
    pub rpc: Option<String>,
    /// Optional REST base URL override.
    #[serde(default)]
    pub provider_base_url: Option<String>,
    /// REST timeout seconds (crate default `2`).
    #[serde(default)]
    pub provider_timeout_seconds: Option<u64>,
    /// Emulation timeout seconds (crate default `10`).
    #[serde(default)]
    pub provider_emulation_timeout_seconds: Option<u64>,
    /// Wallet workchain (crate default `0`).
    #[serde(default)]
    pub workchain: Option<i32>,
    /// Optional batcher idle flush interval. Set in TOML → camelCase scheme JSON.
    #[serde(default)]
    pub batch_flush_interval_seconds: Option<u64>,
    /// Optional queue length that triggers a flush. Set in TOML → camelCase scheme JSON.
    #[serde(default)]
    pub batch_flush_size: Option<u64>,
    /// Optional trace confirmation timeout. Set in TOML → camelCase scheme JSON.
    #[serde(default)]
    pub confirmation_timeout_seconds: Option<u64>,
}

/// Full TVM chain configuration with chain reference.
#[derive(Debug, Clone)]
pub(crate) struct TvmChainConfig {
    /// `-239` (mainnet) or `-3` (testnet).
    pub chain_reference: TvmChainReference,
    /// TOML-level configuration.
    pub inner: TvmChainConfigInner,
}

impl TvmChainConfig {
    /// Parse a CAIP-2 keyed chain table.
    ///
    /// # Errors
    ///
    /// Returns an error if the reference is not `-239`/`-3`, `rpc` is present
    /// but not a string, or the table does not match [`TvmChainConfigInner`].
    pub(crate) fn from_toml(chain_id: &ChainId, value: toml::Value) -> Result<Self, AppError> {
        let chain_reference = TvmChainReference::try_from(chain_id.clone())
            .map_err(|e| AppError::config_with(format!("invalid chain id '{chain_id}'"), e))?;
        require_string_rpc(chain_id, &value)?;
        let inner: TvmChainConfigInner = value.try_into().map_err(|e: toml::de::Error| {
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

    /// Scheme JSON: camelCase keys only when TOML set them.
    ///
    /// Do not `to_value(TvmExactFacilitatorConfig)` (Deserialize-only).
    #[must_use]
    pub(crate) fn scheme_config_json(&self) -> Option<serde_json::Value> {
        let mut obj = serde_json::Map::new();
        if let Some(v) = self.inner.batch_flush_interval_seconds {
            obj.insert("batchFlushIntervalSeconds".to_owned(), serde_json::json!(v));
        }
        if let Some(v) = self.inner.batch_flush_size {
            obj.insert("batchFlushSize".to_owned(), serde_json::json!(v));
        }
        if let Some(v) = self.inner.confirmation_timeout_seconds {
            obj.insert(
                "confirmationTimeoutSeconds".to_owned(),
                serde_json::json!(v),
            );
        }
        if obj.is_empty() {
            None
        } else {
            Some(serde_json::Value::Object(obj))
        }
    }
}

/// Build a TVM provider from TOML.
///
/// # Errors
///
/// Returns an error if no signer is configured, the key cannot be parsed, or
/// Highload / REST construction fails.
pub(crate) fn build_tvm_provider(config: &TvmChainConfig) -> Result<TvmChainProvider, AppError> {
    let chain_id = config.chain_id();
    if config.inner.signer.trim().is_empty() {
        return Err(AppError::chain(format!(
            "no signer configured for TVM chain {chain_id}"
        )));
    }

    let mut hl = HighloadV3Config::from_private_key_str(&config.inner.signer)
        .map_err(|e| AppError::chain_with("failed to parse TVM signer", e))?;
    if let Some(api_key) = config.inner.api_key.clone() {
        hl.api_key = Some(api_key);
    }
    if let Some(id) = config.inner.subwallet_id {
        hl.subwallet_id = id;
    }
    if let Some(timeout) = config.inner.highload_timeout {
        hl.timeout = timeout;
    }
    if let Some(amount) = config.inner.relay_amount {
        hl.relay_amount = u128::from(amount);
    }
    if let Some(name) = config.inner.provider.as_deref() {
        hl.provider = TvmProviderKind::parse(name)
            .map_err(|e| AppError::chain_with("invalid TVM provider", e))?;
    }
    if let Some(url) = config
        .inner
        .provider_base_url
        .as_deref()
        .or(config.inner.rpc.as_deref())
    {
        hl.provider_base_url = Some(url.to_owned());
    }
    if let Some(secs) = config.inner.provider_timeout_seconds {
        hl.provider_timeout_seconds = secs;
    }
    if let Some(secs) = config.inner.provider_emulation_timeout_seconds {
        hl.provider_emulation_timeout_seconds = secs;
    }
    if let Some(workchain) = config.inner.workchain {
        hl.workchain = workchain;
    }

    TvmChainProvider::new(config.chain_reference, hl)
        .map_err(|e| AppError::chain_with("TVM provider init failed", e))
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use r402_core::chain::ChainProvider;

    use super::*;

    fn chain_id() -> ChainId {
        ChainId::from_str("tvm:-3").unwrap()
    }

    const ZERO_SEED_HEX: &str = "0000000000000000000000000000000000000000000000000000000000000001";

    #[test]
    fn from_toml_rejects_evm_shaped_rpc() {
        let value = toml::from_str(r#"rpc = [{ http = "https://example.com" }]"#).unwrap();
        let err = TvmChainConfig::from_toml(&chain_id(), value).unwrap_err();
        assert!(
            err.to_string().contains("`rpc` must be a string URL"),
            "got {err}"
        );
    }

    #[test]
    fn scheme_json_omitted_when_unset() {
        let value = toml::from_str(r#"signer = "00""#).unwrap();
        let config = TvmChainConfig::from_toml(&chain_id(), value).unwrap();
        assert_eq!(config.scheme_config_json(), None, "default omit");
        assert_eq!(config.inner.rpc, None, "rpc optional");
    }

    #[test]
    fn scheme_json_only_set_keys() {
        let value = toml::from_str(
            r#"
signer = "00"
batch_flush_interval_seconds = 2
confirmation_timeout_seconds = 15
"#,
        )
        .unwrap();
        let config = TvmChainConfig::from_toml(&chain_id(), value).unwrap();
        assert_eq!(
            config.scheme_config_json(),
            Some(serde_json::json!({
                "batchFlushIntervalSeconds": 2,
                "confirmationTimeoutSeconds": 15
            })),
            "camelCase set keys only"
        );
    }

    #[test]
    fn build_accepts_hex_seed() {
        let value = toml::from_str(&format!(
            "signer = \"{ZERO_SEED_HEX}\"\nrpc = \"https://testnet.toncenter.com\""
        ))
        .unwrap();
        let config = TvmChainConfig::from_toml(&chain_id(), value).unwrap();
        let provider = build_tvm_provider(&config).unwrap();
        assert_eq!(provider.chain_id().to_string(), "tvm:-3", "testnet");
    }

    #[test]
    fn build_rejects_empty_signer() {
        let value = toml::from_str(r#"signer = """#).unwrap();
        let config = TvmChainConfig::from_toml(&chain_id(), value).unwrap();
        let err = build_tvm_provider(&config).unwrap_err();
        assert!(
            err.to_string().contains("no signer configured"),
            "got {err}"
        );
    }

    #[test]
    fn build_rejects_unknown_provider() {
        let value = toml::from_str(&format!(
            "signer = \"{ZERO_SEED_HEX}\"\nprovider = \"not-a-provider\""
        ))
        .unwrap();
        let config = TvmChainConfig::from_toml(&chain_id(), value).unwrap();
        let err = build_tvm_provider(&config).unwrap_err();
        assert!(
            err.to_string().contains("invalid TVM provider"),
            "got {err}"
        );
    }
}
