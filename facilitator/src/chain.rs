//! Blockchain chain types, configuration, and provider registry.
//!
//! This module centralises everything related to blockchain networks:
//!
//! - **Configuration** — [`ChainConfig`] / [`ChainsConfig`] with CAIP-2 keyed
//!   TOML (de)serialisation.
//! - **Providers** — [`ChainProvider`] enum wrapping chain-family–specific RPC
//!   providers, plus factory functions.
//! - **Registry** — [`ChainRegistry`] initialisation from [`ChainsConfig`].

use std::collections::HashMap;
use std::ops::Deref;
#[cfg(any(feature = "chain-eip155", feature = "chain-solana"))]
use std::sync::Arc;

use r402::chain::{ChainId, ChainProvider as ChainProviderTrait, ChainRegistry};
#[cfg(feature = "chain-eip155")]
use r402_evm::chain as eip155;
#[cfg(feature = "chain-eip155")]
use r402_evm::chain::Eip155ChainReference;
#[cfg(feature = "chain-solana")]
use r402_svm::chain as solana;
#[cfg(feature = "chain-solana")]
use r402_svm::chain::SolanaChainReference;
use serde::{Deserialize, Serialize};

/// Single RPC endpoint entry for EVM chains.
#[cfg(feature = "chain-eip155")]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Eip155RpcEndpoint {
    /// HTTP(S) RPC URL.
    pub http: String,
    /// Optional per-endpoint rate limit (requests/second).
    #[serde(default)]
    pub rate_limit: Option<u32>,
}

/// Inner configuration for an EVM chain (matches TOML structure).
#[cfg(feature = "chain-eip155")]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Eip155ChainConfigInner {
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
    /// Transaction receipt timeout in seconds (default: 30).
    #[serde(default = "default_receipt_timeout")]
    pub receipt_timeout_secs: u64,
}

#[cfg(feature = "chain-eip155")]
fn default_true() -> bool {
    true
}

#[cfg(feature = "chain-eip155")]
fn default_receipt_timeout() -> u64 {
    30
}

/// Full EVM chain configuration with chain reference.
#[cfg(feature = "chain-eip155")]
#[derive(Debug, Clone)]
pub struct Eip155ChainConfig {
    /// Numeric EIP-155 chain reference.
    pub chain_reference: Eip155ChainReference,
    /// TOML-level configuration.
    pub inner: Eip155ChainConfigInner,
}

#[cfg(feature = "chain-eip155")]
impl Eip155ChainConfig {
    /// Returns the CAIP-2 chain ID for this configuration.
    #[must_use]
    pub fn chain_id(&self) -> ChainId {
        self.chain_reference.into()
    }
}

/// Inner configuration for a Solana chain (matches TOML structure).
#[cfg(feature = "chain-solana")]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SolanaChainConfigInner {
    /// RPC endpoint URL.
    pub rpc: String,
    /// Optional WebSocket pubsub endpoint URL.
    #[serde(default)]
    pub pubsub: Option<String>,
    /// Signer private key (base58, 64-byte keypair). Injected by the signers preprocessor.
    #[serde(default)]
    pub signer: Option<String>,
    /// Maximum compute units per transaction (default: 200_000).
    #[serde(default = "default_compute_unit_limit")]
    pub max_compute_unit_limit: u32,
    /// Maximum price per compute unit in micro-lamports (default: 1_000_000).
    #[serde(default = "default_compute_unit_price")]
    pub max_compute_unit_price: u64,
}

#[cfg(feature = "chain-solana")]
fn default_compute_unit_limit() -> u32 {
    200_000
}

#[cfg(feature = "chain-solana")]
fn default_compute_unit_price() -> u64 {
    1_000_000
}

/// Full Solana chain configuration with chain reference.
#[cfg(feature = "chain-solana")]
#[derive(Debug, Clone)]
pub struct SolanaChainConfig {
    /// Solana genesis hash chain reference.
    pub chain_reference: SolanaChainReference,
    /// TOML-level configuration.
    pub inner: SolanaChainConfigInner,
}

#[cfg(feature = "chain-solana")]
impl SolanaChainConfig {
    /// Returns the CAIP-2 chain ID for this configuration.
    #[must_use]
    pub fn chain_id(&self) -> ChainId {
        self.chain_reference.into()
    }
}

/// Chain-specific configuration variant.
///
/// Selected by the CAIP-2 namespace prefix of the chain identifier key
/// (e.g. `"eip155:"` → EVM, `"solana:"` → Solana).
#[derive(Debug, Clone)]
pub enum ChainConfig {
    /// EVM chain configuration (for chains with `"eip155:"` prefix).
    #[cfg(feature = "chain-eip155")]
    Eip155(Box<Eip155ChainConfig>),
    /// Solana chain configuration (for chains with `"solana:"` prefix).
    #[cfg(feature = "chain-solana")]
    Solana(Box<SolanaChainConfig>),
}

/// Ordered collection of [`ChainConfig`] entries.
///
/// Serialised as a TOML map keyed by CAIP-2 chain identifiers.
#[derive(Debug, Clone, Default)]
pub struct ChainsConfig(pub Vec<ChainConfig>);

impl Deref for ChainsConfig {
    type Target = Vec<ChainConfig>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Serialize for ChainsConfig {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeMap;

        let chains = &self.0;
        #[allow(unused_mut)] // For when no chain features enabled
        let mut map = serializer.serialize_map(Some(chains.len()))?;
        for chain_config in chains {
            match chain_config {
                #[cfg(feature = "chain-eip155")]
                ChainConfig::Eip155(config) => {
                    map.serialize_entry(&config.chain_id(), &config.inner)?;
                }
                #[cfg(feature = "chain-solana")]
                ChainConfig::Solana(config) => {
                    map.serialize_entry(&config.chain_id(), &config.inner)?;
                }
                #[allow(unreachable_patterns)]
                _ => unreachable!("ChainConfig variant not enabled in this build"),
            }
        }
        map.end()
    }
}

impl<'de> Deserialize<'de> for ChainsConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use std::fmt;

        use serde::de::{MapAccess, Visitor};

        struct ChainsVisitor;

        impl<'de> Visitor<'de> for ChainsVisitor {
            type Value = ChainsConfig;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a map of chain identifiers to chain configurations")
            }

            fn visit_map<M>(self, mut access: M) -> Result<Self::Value, M::Error>
            where
                M: MapAccess<'de>,
            {
                #[allow(unused_mut)]
                let mut chains = Vec::with_capacity(access.size_hint().unwrap_or(0));

                while let Some(chain_id) = access.next_key::<ChainId>()? {
                    let namespace = chain_id.namespace();
                    #[allow(unused_variables)]
                    let config = match namespace {
                        #[cfg(feature = "chain-eip155")]
                        eip155::EIP155_NAMESPACE => {
                            let inner: Eip155ChainConfigInner = access.next_value()?;
                            let config = Eip155ChainConfig {
                                chain_reference: chain_id
                                    .try_into()
                                    .map_err(|e| serde::de::Error::custom(format!("{e}")))?,
                                inner,
                            };
                            ChainConfig::Eip155(Box::new(config))
                        }
                        #[cfg(feature = "chain-solana")]
                        solana::SOLANA_NAMESPACE => {
                            let inner: SolanaChainConfigInner = access.next_value()?;
                            let config = SolanaChainConfig {
                                chain_reference: chain_id
                                    .try_into()
                                    .map_err(|e| serde::de::Error::custom(format!("{e}")))?,
                                inner,
                            };
                            ChainConfig::Solana(Box::new(config))
                        }
                        _ => {
                            return Err(serde::de::Error::custom(format!(
                                "Unexpected namespace: {namespace}"
                            )));
                        }
                    };
                    #[allow(unreachable_code)]
                    chains.push(config);
                }

                Ok(ChainsConfig(chains))
            }
        }

        deserializer.deserialize_map(ChainsVisitor)
    }
}

/// Unified blockchain provider wrapping chain-family–specific implementations.
#[derive(Debug, Clone)]
pub enum ChainProvider {
    /// EVM chain provider for EIP-155 compatible networks.
    #[cfg(feature = "chain-eip155")]
    Eip155(Arc<eip155::Eip155ChainProvider>),
    /// Solana chain provider.
    #[cfg(feature = "chain-solana")]
    Solana(Arc<solana::SolanaChainProvider>),
}

impl ChainProviderTrait for ChainProvider {
    fn signer_addresses(&self) -> Vec<String> {
        match self {
            #[cfg(feature = "chain-eip155")]
            Self::Eip155(provider) => provider.signer_addresses(),
            #[cfg(feature = "chain-solana")]
            Self::Solana(provider) => provider.signer_addresses(),
            #[allow(unreachable_patterns)]
            _ => unreachable!("ChainProvider variant not enabled in this build"),
        }
    }

    fn chain_id(&self) -> ChainId {
        match self {
            #[cfg(feature = "chain-eip155")]
            Self::Eip155(provider) => provider.chain_id(),
            #[cfg(feature = "chain-solana")]
            Self::Solana(provider) => provider.chain_id(),
            #[allow(unreachable_patterns)]
            _ => unreachable!("ChainProvider variant not enabled in this build"),
        }
    }
}
/// Create a [`ChainProvider`] from a single [`ChainConfig`] entry.
///
/// # Errors
///
/// Returns an error if the provider cannot be constructed (e.g. invalid keys,
/// RPC connection failure).
pub async fn build_chain_provider(
    config: &ChainConfig,
) -> Result<ChainProvider, Box<dyn std::error::Error>> {
    #[allow(unused_variables)]
    let provider = match config {
        #[cfg(feature = "chain-eip155")]
        ChainConfig::Eip155(config) => {
            use alloy_network::EthereumWallet;
            use alloy_signer_local::PrivateKeySigner;
            use url::Url;

            let mut signers: Vec<PrivateKeySigner> = Vec::new();
            for key_hex in &config.inner.signers {
                let signer: PrivateKeySigner = key_hex
                    .parse()
                    .map_err(|e| format!("Failed to parse EVM signer key: {e}"))?;
                signers.push(signer);
            }
            if signers.is_empty() {
                return Err(
                    format!("No signers configured for EVM chain {}", config.chain_id()).into(),
                );
            }

            let wallet = if signers.len() == 1 {
                EthereumWallet::from(signers.into_iter().next().expect("checked non-empty"))
            } else {
                let mut w = EthereumWallet::from(signers[0].clone());
                for s in &signers[1..] {
                    w.register_signer(s.clone());
                }
                w
            };

            let endpoints: Vec<(Url, Option<u32>)> = config
                .inner
                .rpc
                .iter()
                .filter_map(|ep| Url::parse(&ep.http).ok().map(|url| (url, ep.rate_limit)))
                .collect();

            let provider = eip155::Eip155ChainProvider::new(
                config.chain_reference,
                wallet,
                &endpoints,
                config.inner.eip1559,
                config.inner.flashblocks,
                config.inner.receipt_timeout_secs,
            )?;
            ChainProvider::Eip155(Arc::new(provider))
        }
        #[cfg(feature = "chain-solana")]
        ChainConfig::Solana(config) => {
            use solana_keypair::Keypair;

            let signer_str = config.inner.signer.as_ref().ok_or_else(|| {
                format!(
                    "No signer configured for Solana chain {}",
                    config.chain_id()
                )
            })?;
            let keypair_bytes = bs58::decode(signer_str)
                .into_vec()
                .map_err(|e| format!("Failed to decode Solana signer key: {e}"))?;
            // solana-keypair v3: construct from 32-byte secret key array
            let secret_bytes: [u8; 32] = keypair_bytes
                .get(..32)
                .and_then(|s| s.try_into().ok())
                .ok_or_else(|| {
                    format!(
                        "Solana signer key must be at least 32 bytes, got {}",
                        keypair_bytes.len()
                    )
                })?;
            let keypair = Keypair::new_from_array(secret_bytes);

            let provider = solana::SolanaChainProvider::new(
                keypair,
                config.inner.rpc.clone(),
                config.inner.pubsub.clone(),
                config.chain_reference,
                config.inner.max_compute_unit_limit,
                config.inner.max_compute_unit_price,
            )
            .await
            .map_err(|e| format!("Failed to create Solana provider: {e}"))?;
            ChainProvider::Solana(Arc::new(provider))
        }
        #[allow(unreachable_patterns)]
        _ => unreachable!("ChainConfig variant not enabled in this build"),
    };
    #[allow(unreachable_code)]
    Ok(provider)
}

/// Build a [`ChainRegistry`] from a [`ChainsConfig`].
///
/// # Errors
///
/// Returns an error if any chain provider fails to initialise.
pub async fn build_chain_registry(
    chains: &ChainsConfig,
) -> Result<ChainRegistry<ChainProvider>, Box<dyn std::error::Error>> {
    let mut providers = HashMap::new();
    for chain in chains.iter() {
        let chain_provider = build_chain_provider(chain).await?;
        providers.insert(chain_provider.chain_id(), chain_provider);
    }
    Ok(ChainRegistry::new(providers))
}
