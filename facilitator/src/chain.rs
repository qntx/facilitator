//! Blockchain chain types, configuration, and provider registry.
//!
//! This module centralises everything related to blockchain networks:
//!
//! - **Configuration** — [`ChainConfig`] / [`ChainsConfig`] with CAIP-2 keyed
//!   TOML (de)serialisation.
//! - **Providers** — [`ChainProvider`] enum wrapping chain-family–specific RPC
//!   providers, plus [`FromConfig`] factories.
//! - **Registry** — [`ChainRegistry`] initialisation from [`ChainsConfig`].

use r402::chain::{ChainId, ChainProviderOps, ChainRegistry, FromConfig};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::ops::Deref;

#[cfg(feature = "chain-eip155")]
use r402_evm::chain as eip155;
#[cfg(feature = "chain-eip155")]
use r402_evm::chain::config::{Eip155ChainConfig, Eip155ChainConfigInner};
#[cfg(feature = "chain-solana")]
use r402_svm::chain as solana;
#[cfg(feature = "chain-solana")]
use r402_svm::chain::config::{SolanaChainConfig, SolanaChainConfigInner};
#[cfg(any(feature = "chain-eip155", feature = "chain-solana"))]
use std::sync::Arc;

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
        use serde::de::{MapAccess, Visitor};
        use std::fmt;

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

#[async_trait::async_trait]
impl FromConfig<ChainConfig> for ChainProvider {
    async fn from_config(chains: &ChainConfig) -> Result<Self, Box<dyn std::error::Error>> {
        #[allow(unused_variables)]
        let provider = match chains {
            #[cfg(feature = "chain-eip155")]
            ChainConfig::Eip155(config) => {
                let provider = eip155::Eip155ChainProvider::from_config(config).await?;
                Self::Eip155(Arc::new(provider))
            }
            #[cfg(feature = "chain-solana")]
            ChainConfig::Solana(config) => {
                let provider = solana::SolanaChainProvider::from_config(config).await?;
                Self::Solana(Arc::new(provider))
            }
            #[allow(unreachable_patterns)]
            _ => unreachable!("ChainConfig variant not enabled in this build"),
        };
        #[allow(unreachable_code)]
        Ok(provider)
    }
}

impl ChainProviderOps for ChainProvider {
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

#[async_trait::async_trait]
impl FromConfig<ChainsConfig> for ChainRegistry<ChainProvider> {
    async fn from_config(chains: &ChainsConfig) -> Result<Self, Box<dyn std::error::Error>> {
        let mut providers = HashMap::new();
        for chain in chains.iter() {
            let chain_provider = ChainProvider::from_config(chain).await?;
            providers.insert(chain_provider.chain_id(), chain_provider);
        }
        Ok(Self::new(providers))
    }
}
