//! Keeta provider + exact facilitator wiring.

use std::sync::Arc;

use base64::Engine;
use compact_str::CompactString;
use r402_facilitator::SettlementCache;
use r402_keeta::KeetaExactFacilitator;
use r402_keeta::chain::{KeetaChainProvider, KeetaChainReference, KeetaFeePayer};
use r402_keeta::exact::facilitator::SettlementQueue;
use r402_protocol::ExactScheme;
use r402_protocol::scheme::SchemeSlug;

use super::{FacilitatorMap, named_secret, scheme_not_enabled};
use crate::config::{Config, KeetaNetwork};
use crate::error::Error;

/// Register Keeta exact for `network`.
pub(super) fn register(
    map: &mut FacilitatorMap,
    network: &KeetaNetwork,
    config: &Config,
    lookup: &impl Fn(&str) -> Option<String>,
    cache: &SettlementCache,
) -> Result<(), Error> {
    let provider = provider(network, config, lookup)?;
    for name in &network.schemes {
        match name.as_str() {
            ExactScheme::VALUE => {
                // Duplicate try_new's queue construction so the process cache is shared.
                let accounts = provider
                    .fee_payers()
                    .iter()
                    .map(|payer| Arc::clone(payer.account()))
                    .collect();
                let queue =
                    SettlementQueue::new(accounts, provider.chain_reference().client_network());
                let facilitator = KeetaExactFacilitator::with_settlement_cache(
                    provider.clone(),
                    queue,
                    cache.clone(),
                );
                insert_scheme(map, network, ExactScheme::VALUE, facilitator)?;
            }
            _ => return Err(scheme_not_enabled(name, &network.chain_id)),
        }
    }
    Ok(())
}

fn provider(
    network: &KeetaNetwork,
    config: &Config,
    lookup: &impl Fn(&str) -> Option<String>,
) -> Result<KeetaChainProvider, Error> {
    let chain = KeetaChainReference::try_from(network.chain_id.clone()).map_err(|err| {
        Error::config_with(
            format!("invalid Keeta chain id '{}'", network.chain_id),
            err,
        )
    })?;
    let secret = named_secret(config, &network.signer, lookup)?;
    let seed = keeta_seed(&network.signer, &secret)?;
    let mut fee_payers = Vec::with_capacity(network.indices.len());
    for index in &network.indices {
        fee_payers.push(
            KeetaFeePayer::from_ed25519_seed(seed, *index).map_err(|err| {
                Error::config_with(
                    format!(
                        "signer '{}' index {index} is not a valid Keeta ed25519 seed",
                        network.signer
                    ),
                    err,
                )
            })?,
        );
    }
    KeetaChainProvider::new(chain, fee_payers).map_err(|err| {
        Error::config_with(
            format!(
                "failed to build KeetaChainProvider for '{}'",
                network.chain_id
            ),
            err,
        )
    })
}

fn keeta_seed(name: &str, secret: &str) -> Result<[u8; 32], Error> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(secret.trim())
        .map_err(|err| {
            Error::config_with(
                format!("signer '{name}' is not a valid Keeta base64 seed"),
                err,
            )
        })?;
    <[u8; 32]>::try_from(bytes).map_err(|bytes| {
        Error::config(format!(
            "signer '{name}' is not a valid Keeta base64 seed (decoded {} bytes, expected 32)",
            bytes.len()
        ))
    })
}

fn insert_scheme(
    map: &mut FacilitatorMap,
    network: &KeetaNetwork,
    name: &'static str,
    handler: KeetaExactFacilitator<KeetaChainProvider>,
) -> Result<(), Error> {
    let slug = SchemeSlug::new(network.chain_id.clone(), CompactString::from(name));
    map.insert(slug, Arc::new(handler))
}
