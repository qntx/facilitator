//! Chain provider types and registry construction.

use std::collections::HashMap;
#[cfg(any(feature = "chain-eip155", feature = "chain-solana"))]
use std::sync::Arc;

use r402::chain::{ChainId, ChainProvider as ChainProviderTrait, ChainRegistry};
#[cfg(feature = "chain-eip155")]
use r402_evm::chain as eip155;
#[cfg(feature = "chain-solana")]
use r402_svm::chain as solana;

use super::config::{ChainConfig, ChainsConfig};
use crate::error::Error;

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
/// Delegates to chain-family–specific builder functions, each gated behind
/// the corresponding feature flag.
///
/// # Errors
///
/// Returns an error if the provider cannot be constructed (e.g. invalid keys,
/// RPC connection failure).
pub async fn build_chain_provider(config: &ChainConfig) -> Result<ChainProvider, Error> {
    match config {
        #[cfg(feature = "chain-eip155")]
        ChainConfig::Eip155(config) => build_eip155_provider(config),
        #[cfg(feature = "chain-solana")]
        ChainConfig::Solana(config) => build_solana_provider(config).await,
        #[allow(unreachable_patterns)]
        _ => unreachable!("ChainConfig variant not enabled in this build"),
    }
}

/// Build an EVM (EIP-155) chain provider from the given configuration.
///
/// # Errors
///
/// Returns an error if signer keys cannot be parsed, no signers are
/// configured, or the underlying RPC provider fails to initialise.
#[cfg(feature = "chain-eip155")]
fn build_eip155_provider(
    config: &super::config::Eip155ChainConfig,
) -> Result<ChainProvider, Error> {
    use alloy_network::EthereumWallet;
    use alloy_signer_local::PrivateKeySigner;
    use url::Url;

    let signers: Vec<PrivateKeySigner> = config
        .inner
        .signers
        .iter()
        .map(|k| {
            k.parse()
                .map_err(|e| Error::chain(format!("failed to parse EVM signer key: {e}")))
        })
        .collect::<Result<_, _>>()?;

    if signers.is_empty() {
        return Err(Error::chain(format!(
            "no signers configured for EVM chain {}",
            config.chain_id()
        )));
    }

    let mut iter = signers.into_iter();
    let mut wallet = EthereumWallet::from(iter.next().expect("checked non-empty"));
    for s in iter {
        wallet.register_signer(s);
    }

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
    )
    .map_err(|e| Error::chain(format!("EVM provider init failed: {e}")))?;

    Ok(ChainProvider::Eip155(Arc::new(provider)))
}

/// Build a Solana chain provider from the given configuration.
///
/// # Errors
///
/// Returns an error if the signer key is missing, cannot be base58-decoded,
/// is too short, or the RPC connection fails.
#[cfg(feature = "chain-solana")]
async fn build_solana_provider(
    config: &super::config::SolanaChainConfig,
) -> Result<ChainProvider, Error> {
    use solana_keypair::Keypair;

    let signer_str = config.inner.signer.as_ref().ok_or_else(|| {
        Error::chain(format!(
            "no signer configured for Solana chain {}",
            config.chain_id()
        ))
    })?;

    let keypair_bytes = bs58::decode(signer_str)
        .into_vec()
        .map_err(|e| Error::chain_with("failed to decode Solana signer key", e))?;

    // solana-keypair v3: construct from 32-byte secret key array
    let secret_bytes: [u8; 32] = keypair_bytes
        .get(..32)
        .and_then(|s| s.try_into().ok())
        .ok_or_else(|| {
            Error::chain(format!(
                "Solana signer key must be at least 32 bytes, got {}",
                keypair_bytes.len()
            ))
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
    .map_err(|e| Error::chain(format!("failed to create Solana provider: {e}")))?;

    Ok(ChainProvider::Solana(Arc::new(provider)))
}

/// Build a [`ChainRegistry`] from a [`ChainsConfig`].
///
/// # Errors
///
/// Returns an error if any chain provider fails to initialise.
pub async fn build_chain_registry(
    chains: &ChainsConfig,
) -> Result<ChainRegistry<ChainProvider>, Error> {
    let mut providers = HashMap::new();
    for chain in chains.iter() {
        let chain_provider = build_chain_provider(chain).await?;
        providers.insert(chain_provider.chain_id(), chain_provider);
    }
    Ok(ChainRegistry::new(providers))
}
