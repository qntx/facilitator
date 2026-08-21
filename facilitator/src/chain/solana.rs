//! Solana chain handle and provider construction.

use std::sync::Arc;

use r402_core::chain::{ChainId, ChainProvider};
use r402_core::facilitator::DynFacilitator;
use r402_core::scheme::SchemeBuilder;
use r402_solana::SolanaExact;
use r402_solana::chain::{SolanaChainProvider, SolanaChainReference};
use r402_solana::exact::facilitator::SolanaExactFacilitatorConfig;
use serde::Deserialize;
use solana_keypair::Keypair;

use crate::error::AppError;

/// Local handle: r402-solana has no `SchemeBuilder<&SolanaChainProvider>`, which `register` requires.
#[derive(Debug)]
pub(crate) struct SolanaHandle(pub Arc<SolanaChainProvider>);

impl ChainProvider for SolanaHandle {
    fn signer_addresses(&self) -> Vec<String> {
        self.0.signer_addresses()
    }

    fn chain_id(&self) -> ChainId {
        self.0.chain_id()
    }
}

impl SchemeBuilder<&SolanaHandle> for SolanaExact {
    fn build(
        &self,
        provider: &SolanaHandle,
        config: Option<serde_json::Value>,
    ) -> Result<Box<dyn DynFacilitator>, Box<dyn std::error::Error + Send + Sync>> {
        SchemeBuilder::<Arc<SolanaChainProvider>>::build(self, Arc::clone(&provider.0), config)
    }
}

/// Inner configuration for a Solana chain (matches TOML structure).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SolanaChainConfigInner {
    /// HTTP RPC endpoint URL.
    pub rpc: String,
    /// Optional `WebSocket` pubsub endpoint URL.
    #[serde(default)]
    pub pubsub: Option<String>,
    /// Signer private key (base58, 64-byte keypair). Injected by the signers preprocessor.
    #[serde(default)]
    pub signer: Option<String>,
    /// Maximum compute units per transaction (default: `200_000`).
    #[serde(default = "default_compute_unit_limit")]
    pub max_compute_unit_limit: u32,
    /// Maximum price per compute unit in micro-lamports (default: `1_000_000`).
    #[serde(default = "default_compute_unit_price")]
    pub max_compute_unit_price: u64,
    /// Enable Path 2 smart-wallet verification (default: false).
    #[serde(default)]
    pub enable_smart_wallet_verification: bool,
    /// Allow extra instructions beyond `TransferChecked` + compute budget (default: true).
    #[serde(default = "default_allow_additional_instructions")]
    pub allow_additional_instructions: bool,
    /// Maximum instruction count in a client transaction (default: 6).
    #[serde(default = "default_max_instruction_count")]
    pub max_instruction_count: usize,
}

/// Default maximum compute units per Solana transaction.
const fn default_compute_unit_limit() -> u32 {
    200_000
}

/// Default maximum price per compute unit in micro-lamports.
const fn default_compute_unit_price() -> u64 {
    1_000_000
}

/// Default: allow Phantom/Solflare lighthouse extra instructions.
const fn default_allow_additional_instructions() -> bool {
    true
}

/// Default instruction cap from the SVM exact scheme recommendation.
const fn default_max_instruction_count() -> usize {
    6
}

/// Full Solana chain configuration with chain reference.
#[derive(Debug, Clone)]
pub(crate) struct SolanaChainConfig {
    /// Solana genesis-hash chain reference.
    pub chain_reference: SolanaChainReference,
    /// TOML-level configuration.
    pub inner: SolanaChainConfigInner,
}

impl SolanaChainConfig {
    /// Parse a CAIP-2 keyed chain table.
    ///
    /// # Errors
    ///
    /// Returns an error if the reference is not a 32-character Solana genesis
    /// prefix or the table does not match [`SolanaChainConfigInner`].
    pub(crate) fn from_toml(chain_id: &ChainId, value: toml::Value) -> Result<Self, AppError> {
        let chain_reference = SolanaChainReference::try_from(chain_id.clone())
            .map_err(|e| AppError::config_with(format!("invalid chain id '{chain_id}'"), e))?;
        let inner: SolanaChainConfigInner = value.try_into().map_err(|e: toml::de::Error| {
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

    /// Scheme JSON for `SolanaExact`. Serialized from the typed config so keys
    /// are camelCase (`enableSmartWalletVerification`), never `snake_case` TOML.
    ///
    /// # Errors
    ///
    /// Returns an error if the typed config cannot be serialized.
    pub(crate) fn scheme_json(&self) -> Result<serde_json::Value, AppError> {
        let cfg = SolanaExactFacilitatorConfig {
            allow_additional_instructions: self.inner.allow_additional_instructions,
            max_instruction_count: self.inner.max_instruction_count,
            enable_smart_wallet_verification: self.inner.enable_smart_wallet_verification,
            ..SolanaExactFacilitatorConfig::default()
        };
        serde_json::to_value(&cfg)
            .map_err(|e| AppError::chain_with("failed to serialise Solana scheme config", e))
    }
}

/// Build a Solana handle from TOML.
///
/// # Errors
///
/// Returns an error if the signer is missing, cannot be base58-decoded, is
/// shorter than 32 bytes, or the provider fails to initialise (pubsub).
pub(crate) async fn build_solana_handle(
    config: &SolanaChainConfig,
) -> Result<SolanaHandle, AppError> {
    let chain_id = config.chain_id();
    let signer_str = config.inner.signer.as_ref().ok_or_else(|| {
        AppError::chain(format!("no signer configured for Solana chain {chain_id}"))
    })?;

    let keypair_bytes = bs58::decode(signer_str)
        .into_vec()
        .map_err(|e| AppError::chain_with("failed to decode Solana signer key", e))?;

    // solana-keypair signs from the 32-byte seed; base58 keypairs are 64 bytes (seed || pubkey).
    let secret_bytes: [u8; 32] = keypair_bytes
        .get(..32)
        .and_then(|s| s.try_into().ok())
        .ok_or_else(|| {
            AppError::chain(format!(
                "Solana signer key must be at least 32 bytes, got {}",
                keypair_bytes.len()
            ))
        })?;
    let keypair = Keypair::new_from_array(secret_bytes);

    let provider = SolanaChainProvider::new(
        keypair,
        config.inner.rpc.clone(),
        config.inner.pubsub.clone(),
        config.chain_reference,
        config.inner.max_compute_unit_limit,
        config.inner.max_compute_unit_price,
    )
    .await
    .map_err(|e| AppError::chain(format!("failed to create Solana provider: {e}")))?;

    Ok(SolanaHandle(Arc::new(provider)))
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;

    fn parse_inner(toml: &str) -> SolanaChainConfig {
        let chain_id = ChainId::from_str("solana:EtWTRABZaYq6iMfeYKouRu166VU2xqa1").unwrap();
        let value: toml::Value = toml::from_str(toml).unwrap();
        SolanaChainConfig::from_toml(&chain_id, value).unwrap()
    }

    #[test]
    fn scheme_json_uses_camel_case_enable_smart_wallet_verification() {
        let cfg = parse_inner(
            r#"
rpc = "https://api.devnet.solana.com"
enable_smart_wallet_verification = true
allow_additional_instructions = false
max_instruction_count = 4
"#,
        );
        let json = cfg.scheme_json().unwrap();
        assert_eq!(
            json.get("enableSmartWalletVerification"),
            Some(&serde_json::json!(true)),
            "camelCase reaches r402"
        );
        assert_eq!(
            json.get("allowAdditionalInstructions"),
            Some(&serde_json::json!(false)),
            "camelCase"
        );
        assert_eq!(
            json.get("maxInstructionCount"),
            Some(&serde_json::json!(4)),
            "camelCase"
        );
        assert!(
            json.get("enable_smart_wallet_verification").is_none(),
            "must not dump snake_case TOML keys"
        );
    }

    #[test]
    fn scheme_json_defaults_match_r402() {
        let cfg = parse_inner(r#"rpc = "https://api.devnet.solana.com""#);
        let json = cfg.scheme_json().unwrap();
        assert_eq!(
            json.get("enableSmartWalletVerification"),
            Some(&serde_json::json!(false)),
            "default false"
        );
        assert_eq!(
            json.get("allowAdditionalInstructions"),
            Some(&serde_json::json!(true)),
            "default true"
        );
        assert_eq!(
            json.get("maxInstructionCount"),
            Some(&serde_json::json!(6)),
            "default 6"
        );
    }

    #[tokio::test]
    async fn missing_signer_is_startup_error() {
        let cfg = parse_inner(r#"rpc = "https://api.devnet.solana.com""#);
        let err = build_solana_handle(&cfg).await.unwrap_err();
        assert!(
            err.to_string().contains("no signer configured"),
            "got {err}"
        );
    }

    #[tokio::test]
    async fn invalid_base58_signer_is_startup_error() {
        let cfg = parse_inner(
            r#"
rpc = "https://api.devnet.solana.com"
signer = "not-valid-base58!!!"
"#,
        );
        let err = build_solana_handle(&cfg).await.unwrap_err();
        assert!(
            err.to_string()
                .contains("failed to decode Solana signer key"),
            "got {err}"
        );
    }

    #[tokio::test]
    async fn builds_handle_from_64_byte_keypair() {
        let keypair = Keypair::new();
        let signer = bs58::encode(keypair.to_bytes()).into_string();
        let cfg = parse_inner(&format!(
            "rpc = \"https://127.0.0.1:1\"\nsigner = \"{signer}\"\n"
        ));
        let handle = build_solana_handle(&cfg).await.unwrap();
        assert_eq!(
            handle.chain_id().to_string(),
            "solana:EtWTRABZaYq6iMfeYKouRu166VU2xqa1",
            "CAIP-2"
        );
        assert_eq!(handle.signer_addresses().len(), 1, "fee payer advertised");
    }
}
