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
/// # Errors
///
/// Returns an error if the provider cannot be constructed (e.g. invalid keys,
/// RPC connection failure).
pub async fn build_chain_provider(config: &ChainConfig) -> Result<ChainProvider, Error> {
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
                    .map_err(|e| Error::Chain(format!("Failed to parse EVM signer key: {e}")))?;
                signers.push(signer);
            }
            if signers.is_empty() {
                return Err(Error::Chain(format!(
                    "No signers configured for EVM chain {}",
                    config.chain_id()
                )));
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
            )
            .map_err(|e| Error::Chain(format!("EVM provider init error: {e}")))?;
            ChainProvider::Eip155(Arc::new(provider))
        }
        #[cfg(feature = "chain-solana")]
        ChainConfig::Solana(config) => {
            use solana_keypair::Keypair;

            let signer_str = config.inner.signer.as_ref().ok_or_else(|| {
                Error::Chain(format!(
                    "No signer configured for Solana chain {}",
                    config.chain_id()
                ))
            })?;
            let keypair_bytes = bs58::decode(signer_str)
                .into_vec()
                .map_err(|e| Error::Chain(format!("Failed to decode Solana signer key: {e}")))?;
            // solana-keypair v3: construct from 32-byte secret key array
            let secret_bytes: [u8; 32] = keypair_bytes
                .get(..32)
                .and_then(|s| s.try_into().ok())
                .ok_or_else(|| {
                    Error::Chain(format!(
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
            .map_err(|e| Error::Chain(format!("Failed to create Solana provider: {e}")))?;
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
) -> Result<ChainRegistry<ChainProvider>, Error> {
    let mut providers = HashMap::new();
    for chain in chains.iter() {
        let chain_provider = build_chain_provider(chain).await?;
        providers.insert(chain_provider.chain_id(), chain_provider);
    }
    Ok(ChainRegistry::new(providers))
}
