//! Chain configuration types and CAIP-2 keyed TOML (de)serialisation.

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
pub(crate) struct Eip155RpcEndpoint {
    /// HTTP(S) RPC URL.
    pub http: String,
    /// Optional per-endpoint rate limit (requests/second).
    #[serde(default)]
    pub rate_limit: Option<u32>,
}

/// Inner configuration for an EVM chain (matches TOML structure).
#[cfg(feature = "chain-eip155")]
#[derive(Debug, Clone, Serialize, Deserialize)]
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
    /// Transaction receipt timeout in seconds (default: 30).
    #[serde(default = "default_receipt_timeout")]
    pub receipt_timeout_secs: u64,
}

/// Serde default returning `true` (for EIP-1559 opt-in).
#[cfg(feature = "chain-eip155")]
const fn default_true() -> bool {
    true
}

/// Default transaction receipt timeout in seconds.
#[cfg(feature = "chain-eip155")]
const fn default_receipt_timeout() -> u64 {
    30
}

/// Full EVM chain configuration with chain reference.
#[cfg(feature = "chain-eip155")]
#[derive(Debug, Clone)]
pub(crate) struct Eip155ChainConfig {
    /// Numeric EIP-155 chain reference.
    pub chain_reference: Eip155ChainReference,
    /// TOML-level configuration.
    pub inner: Eip155ChainConfigInner,
}

#[cfg(feature = "chain-eip155")]
impl Eip155ChainConfig {
    /// Returns the CAIP-2 chain ID for this configuration.
    #[must_use]
    pub(crate) fn chain_id(&self) -> ChainId {
        self.chain_reference.into()
    }
}

/// Inner configuration for a Solana chain (matches TOML structure).
#[cfg(feature = "chain-solana")]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct SolanaChainConfigInner {
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

/// Default maximum compute units per Solana transaction.
#[cfg(feature = "chain-solana")]
const fn default_compute_unit_limit() -> u32 {
    200_000
}

/// Default maximum price per compute unit in micro-lamports.
#[cfg(feature = "chain-solana")]
const fn default_compute_unit_price() -> u64 {
    1_000_000
}

/// Full Solana chain configuration with chain reference.
#[cfg(feature = "chain-solana")]
#[derive(Debug, Clone)]
pub(crate) struct SolanaChainConfig {
    /// Solana genesis hash chain reference.
    pub chain_reference: SolanaChainReference,
    /// TOML-level configuration.
    pub inner: SolanaChainConfigInner,
}

#[cfg(feature = "chain-solana")]
impl SolanaChainConfig {
    /// Returns the CAIP-2 chain ID for this configuration.
    #[must_use]
    pub(crate) fn chain_id(&self) -> ChainId {
        self.chain_reference.into()
    }
}

/// Chain-specific configuration variant.
///
/// Selected by the CAIP-2 namespace prefix of the chain identifier key
/// (e.g. `"eip155:"` → EVM, `"solana:"` → Solana).
#[derive(Debug, Clone)]
pub(crate) enum ChainConfig {
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
pub(crate) struct ChainsConfig(Vec<ChainConfig>);

impl ChainsConfig {
    /// Returns the number of configured chains.
    #[must_use]
    pub(crate) const fn len(&self) -> usize {
        self.0.len()
    }

    /// Returns an iterator over the chain configurations.
    pub(crate) fn iter(&self) -> std::slice::Iter<'_, ChainConfig> {
        self.0.iter()
    }
}

impl Serialize for ChainsConfig {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeMap;

        let chains = &self.0;
        let mut map = serializer.serialize_map(Some(chains.len()))?;
        for chain_config in chains {
            serialize_chain_entry(chain_config, &mut map)?;
        }
        map.end()
    }
}

/// Serialize a single chain config entry into the map.
fn serialize_chain_entry<S: serde::ser::SerializeMap>(
    chain_config: &ChainConfig,
    map: &mut S,
) -> Result<(), S::Error> {
    match chain_config {
        #[cfg(feature = "chain-eip155")]
        ChainConfig::Eip155(config) => map.serialize_entry(&config.chain_id(), &config.inner),
        #[cfg(feature = "chain-solana")]
        ChainConfig::Solana(config) => map.serialize_entry(&config.chain_id(), &config.inner),
    }
}

/// Parse a single CAIP-2 keyed chain entry from a serde map.
fn parse_chain<'de, M: serde::de::MapAccess<'de>>(
    chain_id: ChainId,
    access: &mut M,
) -> Result<ChainConfig, M::Error> {
    match chain_id.namespace() {
        #[cfg(feature = "chain-eip155")]
        eip155::EIP155_NAMESPACE => {
            let inner: Eip155ChainConfigInner = access.next_value()?;
            Ok(ChainConfig::Eip155(Box::new(Eip155ChainConfig {
                chain_reference: chain_id
                    .try_into()
                    .map_err(|e| serde::de::Error::custom(format!("{e}")))?,
                inner,
            })))
        }
        #[cfg(feature = "chain-solana")]
        solana::SOLANA_NAMESPACE => {
            let inner: SolanaChainConfigInner = access.next_value()?;
            Ok(ChainConfig::Solana(Box::new(SolanaChainConfig {
                chain_reference: chain_id
                    .try_into()
                    .map_err(|e| serde::de::Error::custom(format!("{e}")))?,
                inner,
            })))
        }
        unknown => Err(serde::de::Error::custom(format!(
            "Unexpected namespace: {unknown}"
        ))),
    }
}

/// Serde visitor for the CAIP-2 keyed chains map.
struct ChainsVisitor;

impl<'de> serde::de::Visitor<'de> for ChainsVisitor {
    type Value = ChainsConfig;

    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("a map of chain identifiers to chain configurations")
    }

    fn visit_map<M>(self, mut access: M) -> Result<Self::Value, M::Error>
    where
        M: serde::de::MapAccess<'de>,
    {
        let mut chains = Vec::with_capacity(access.size_hint().unwrap_or(0));
        while let Some(chain_id) = access.next_key::<ChainId>()? {
            chains.push(parse_chain(chain_id, &mut access)?);
        }
        Ok(ChainsConfig(chains))
    }
}

impl<'de> Deserialize<'de> for ChainsConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_map(ChainsVisitor)
    }
}
