//! EIP-155 chain handle and provider construction.

use std::sync::Arc;

use alloy_network::EthereumWallet;
use alloy_signer_local::PrivateKeySigner;
use r402_core::chain::{ChainId, ChainProvider};
use r402_core::facilitator::DynFacilitator;
use r402_core::scheme::SchemeBuilder;
use r402_evm::Eip155Exact;
use r402_evm::chain::{Eip155ChainProvider, Eip155ChainReference};
use serde::Deserialize;
use url::Url;

use crate::error::AppError;

/// Local handle: r402-evm has no `SchemeBuilder<&Eip155ChainProvider>`, which `register` requires.
#[derive(Debug)]
pub(crate) struct Eip155Handle(pub Arc<Eip155ChainProvider>);

impl ChainProvider for Eip155Handle {
    fn signer_addresses(&self) -> Vec<String> {
        self.0.signer_addresses()
    }

    fn chain_id(&self) -> ChainId {
        self.0.chain_id()
    }
}

impl SchemeBuilder<&Eip155Handle> for Eip155Exact {
    fn build(
        &self,
        provider: &Eip155Handle,
        config: Option<serde_json::Value>,
    ) -> Result<Box<dyn DynFacilitator>, Box<dyn std::error::Error + Send + Sync>> {
        SchemeBuilder::<Arc<Eip155ChainProvider>>::build(self, Arc::clone(&provider.0), config)
    }
}

/// Single RPC endpoint entry for EVM chains.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Eip155RpcEndpoint {
    /// HTTP(S) RPC URL.
    pub http: String,
    /// Optional per-endpoint rate limit (requests/second).
    #[serde(default)]
    pub rate_limit: Option<u32>,
}

/// Inner configuration for an EVM chain (matches TOML structure).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Eip155ChainConfigInner {
    /// RPC endpoint(s).
    pub rpc: Vec<Eip155RpcEndpoint>,
    /// Signer private keys (hex, 0x-prefixed). Injected by the signers preprocessor.
    #[serde(default)]
    pub signers: Vec<String>,
    /// Whether the chain supports EIP-1559 gas pricing (default: true).
    #[serde(default = "default_true")]
    pub eip1559: bool,
    /// Whether the chain supports flashblocks (default: false).
    #[serde(default)]
    pub flashblocks: bool,
    /// Transaction receipt timeout in seconds (default: 20).
    #[serde(default = "default_receipt_timeout")]
    pub receipt_timeout_secs: u64,
}

/// Serde default returning `true` (for EIP-1559 opt-in).
const fn default_true() -> bool {
    true
}

/// Default EVM receipt wait; must finish inside the 30 s HTTP client budget.
const fn default_receipt_timeout() -> u64 {
    20
}

/// Full EVM chain configuration with chain reference.
#[derive(Debug, Clone)]
pub(crate) struct Eip155ChainConfig {
    /// Numeric EIP-155 chain reference.
    pub chain_reference: Eip155ChainReference,
    /// TOML-level configuration.
    pub inner: Eip155ChainConfigInner,
}

impl Eip155ChainConfig {
    /// Parse a CAIP-2 keyed chain table.
    ///
    /// # Errors
    ///
    /// Returns an error if the reference is not a numeric EIP-155 id or the
    /// table does not match [`Eip155ChainConfigInner`].
    pub(crate) fn from_toml(chain_id: &ChainId, value: toml::Value) -> Result<Self, AppError> {
        let chain_reference = Eip155ChainReference::try_from(chain_id)
            .map_err(|e| AppError::config_with(format!("invalid chain id '{chain_id}'"), e))?;
        let inner: Eip155ChainConfigInner = value.try_into().map_err(|e: toml::de::Error| {
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

/// Build an EIP-155 handle from TOML.
///
/// # Errors
///
/// Returns an error if signer keys cannot be parsed, no signers are
/// configured, or no HTTP RPC remains after URL filtering.
pub(crate) fn build_eip155_handle(config: &Eip155ChainConfig) -> Result<Eip155Handle, AppError> {
    let chain_id = config.chain_id();
    if config.inner.signers.is_empty() {
        return Err(AppError::chain(format!(
            "no signers configured for EVM chain {chain_id}"
        )));
    }

    let mut parsed = Vec::with_capacity(config.inner.signers.len());
    for key in &config.inner.signers {
        let signer = key
            .parse::<PrivateKeySigner>()
            .map_err(|e| AppError::chain_with("failed to parse EVM signer key", e))?;
        parsed.push(signer);
    }
    let mut signers = parsed.into_iter();
    let first = signers.next().ok_or_else(|| {
        AppError::chain(format!("no signers configured for EVM chain {chain_id}"))
    })?;
    let mut wallet = EthereumWallet::from(first);
    for signer in signers {
        wallet.register_signer(signer);
    }

    let endpoints: Vec<(Url, Option<u32>)> = config
        .inner
        .rpc
        .iter()
        .filter_map(|ep| match Url::parse(&ep.http) {
            Ok(url) => Some((url, ep.rate_limit)),
            Err(e) => {
                tracing::warn!(rpc_url = %ep.http, error = %e, "Skipping invalid RPC URL");
                None
            }
        })
        .collect();
    if endpoints.is_empty() {
        return Err(AppError::chain(format!(
            "no HTTP RPC remaining for EVM chain {chain_id}"
        )));
    }

    let provider = Eip155ChainProvider::new(
        config.chain_reference,
        wallet,
        &endpoints,
        config.inner.eip1559,
        config.inner.flashblocks,
        config.inner.receipt_timeout_secs,
    )
    .map_err(|e| AppError::chain(format!("EVM provider init failed: {e}")))?;

    Ok(Eip155Handle(Arc::new(provider)))
}
