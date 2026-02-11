//! Scheme builder implementations for the x402 facilitator.
//!
//! This module provides [`SchemeBuilder`] implementations for all supported
//! payment schemes. These builders create facilitator instances from the generic
//! [`ChainProvider`] enum by extracting the appropriate chain-specific provider.
//!
//! # Supported Schemes
//!
//! | Scheme | Chains | Description |
//! |--------|--------|-------------|
//! | [`Eip155Exact`] | EIP-155 (EVM) | Exact amount payments on EVM |
//! | [`SolanaExact`] | Solana | Exact amount payments on Solana |
//!
//! # Example
//!
//! ```ignore
//! use r402::scheme::SchemeRegistry;
//! use r402_evm::Eip155Exact;
//! use crate::chain::ChainProvider;
//!
//! let mut registry = SchemeRegistry::new();
//! registry.register(&Eip155Exact, &provider, None)?;
//! ```

#[allow(unused_imports)] // For when no chain features are enabled
use std::sync::Arc;

#[allow(unused_imports)] // For when no chain features are enabled
use r402::facilitator::Facilitator;
#[allow(unused_imports)] // For when no chain features are enabled
use r402::scheme::SchemeBuilder;
#[cfg(feature = "chain-eip155")]
use r402_evm::Eip155Exact;
#[cfg(feature = "chain-solana")]
use r402_svm::SolanaExact;

#[allow(unused_imports)] // For when no chain features are enabled
use crate::chain::ChainProvider;

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
