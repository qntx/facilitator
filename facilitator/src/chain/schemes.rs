//! [`SchemeBuilder`] implementations bridging [`ChainProvider`] to scheme handlers.

#[cfg(any(feature = "chain-eip155", feature = "chain-solana"))]
use std::sync::Arc;

#[cfg(any(feature = "chain-eip155", feature = "chain-solana"))]
use r402::facilitator::Facilitator;
#[cfg(any(feature = "chain-eip155", feature = "chain-solana"))]
use r402::scheme::SchemeBuilder;
#[cfg(feature = "chain-eip155")]
use r402_evm::Eip155Exact;
#[cfg(feature = "chain-solana")]
use r402_svm::SolanaExact;

use super::ChainProvider;

#[cfg(feature = "chain-eip155")]
impl SchemeBuilder<&ChainProvider> for Eip155Exact {
    fn build(
        &self,
        provider: &ChainProvider,
        config: Option<serde_json::Value>,
    ) -> Result<Box<dyn Facilitator>, Box<dyn std::error::Error>> {
        #[allow(irrefutable_let_patterns)]
        let eip155_provider = if let ChainProvider::Eip155(provider) = provider {
            Arc::clone(provider)
        } else {
            return Err("Eip155Exact::build: provider must be an Eip155ChainProvider".into());
        };
        self.build(eip155_provider, config)
    }
}

#[cfg(feature = "chain-solana")]
impl SchemeBuilder<&ChainProvider> for SolanaExact {
    fn build(
        &self,
        provider: &ChainProvider,
        config: Option<serde_json::Value>,
    ) -> Result<Box<dyn Facilitator>, Box<dyn std::error::Error>> {
        #[allow(irrefutable_let_patterns)]
        let solana_provider = if let ChainProvider::Solana(provider) = provider {
            Arc::clone(provider)
        } else {
            return Err("SolanaExact::build: provider must be a SolanaChainProvider".into());
        };
        self.build(solana_provider, config)
    }
}
