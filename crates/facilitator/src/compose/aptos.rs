//! Aptos provider + exact facilitator wiring.

use std::sync::Arc;

use compact_str::CompactString;
use r402_aptos::chain::AptosChainReference;
use r402_aptos::{AptosChainProvider, AptosExactFacilitator, AptosFeePayer};
use r402_facilitator::SettlementCache;
use r402_protocol::ExactScheme;
use r402_protocol::scheme::SchemeSlug;

use super::{FacilitatorMap, named_secret, scheme_not_enabled};
use crate::config::{AptosNetwork, Config, resolve_optional_rpc};
use crate::error::Error;

/// Register Aptos exact for `network`.
pub(super) fn register(
    map: &mut FacilitatorMap,
    network: &AptosNetwork,
    config: &Config,
    lookup: &impl Fn(&str) -> Option<String>,
    cache: &SettlementCache,
) -> Result<(), Error> {
    let provider = provider(network, config, lookup)?;
    let sponsor = provider.sponsor_transactions();
    for name in &network.schemes {
        match name.as_str() {
            ExactScheme::VALUE => {
                let facilitator = AptosExactFacilitator::with_settlement_cache(
                    provider.clone(),
                    sponsor,
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
    network: &AptosNetwork,
    config: &Config,
    lookup: &impl Fn(&str) -> Option<String>,
) -> Result<AptosChainProvider, Error> {
    let chain = AptosChainReference::try_from(network.chain_id.clone()).map_err(|err| {
        Error::config_with(
            format!("invalid Aptos chain id '{}'", network.chain_id),
            err,
        )
    })?;
    let rpc_url = resolve_optional_rpc(&network.chain_id, network.rpc.as_ref(), lookup)?;
    let mut fee_payers = Vec::with_capacity(network.fee_payers.len());
    for name in &network.fee_payers {
        let secret = named_secret(config, name, lookup)?;
        fee_payers.push(AptosFeePayer::from_private_key_hex(&secret).map_err(|err| {
            Error::config_with(
                format!("signer '{name}' is not a valid Aptos private key"),
                err,
            )
        })?);
    }
    let provider =
        AptosChainProvider::new(chain, fee_payers, rpc_url.as_deref()).map_err(|err| {
            Error::config_with(
                format!(
                    "failed to build AptosChainProvider for '{}'",
                    network.chain_id
                ),
                err,
            )
        })?;
    Ok(provider.with_sponsor_transactions(network.sponsor_transactions))
}

fn insert_scheme(
    map: &mut FacilitatorMap,
    network: &AptosNetwork,
    name: &'static str,
    handler: AptosExactFacilitator<AptosChainProvider>,
) -> Result<(), Error> {
    let slug = SchemeSlug::new(network.chain_id.clone(), CompactString::from(name));
    map.insert(slug, Arc::new(handler))
}
