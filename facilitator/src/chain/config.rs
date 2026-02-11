//! Chain configuration types and CAIP-2 keyed TOML (de)serialisation.

use std::ops::Deref;

use r402::chain::ChainId;
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
const fn default_true() -> bool {
    true
}

#[cfg(feature = "chain-eip155")]
const fn default_receipt_timeout() -> u64 {
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
}

#[cfg(feature = "chain-solana")]
const fn default_compute_unit_limit() -> u32 {
    200_000
}

#[cfg(feature = "chain-solana")]
const fn default_compute_unit_price() -> u64 {
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
        #[allow(unused_mut)]
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
