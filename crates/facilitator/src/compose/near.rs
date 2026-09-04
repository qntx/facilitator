//! NEAR provider + exact facilitator wiring.

use std::sync::Arc;

use compact_str::CompactString;
use r402_near::NearExactFacilitator;
use r402_near::chain::{NearChainProvider, NearChainReference, NearRelayer};
use r402_near::exact::facilitator::SettlementCache as NearSettlementCache;
use r402_protocol::ExactScheme;
use r402_protocol::scheme::SchemeSlug;

use super::{FacilitatorMap, named_secret, scheme_not_enabled};
use crate::config::{Config, NearNetwork, resolve_optional_rpc};
use crate::error::Error;

/// Register NEAR exact for `network`.
pub(super) fn register(
    map: &mut FacilitatorMap,
    network: &NearNetwork,
    config: &Config,
    lookup: &impl Fn(&str) -> Option<String>,
    cache: &NearSettlementCache,
) -> Result<(), Error> {
    let provider = provider(network, config, lookup)?;
    let gas = provider.max_sponsored_gas();
    for name in &network.schemes {
        match name.as_str() {
            ExactScheme::VALUE => {
                let facilitator = NearExactFacilitator::with_settlement_cache(
                    provider.clone(),
                    gas,
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
    network: &NearNetwork,
    config: &Config,
    lookup: &impl Fn(&str) -> Option<String>,
) -> Result<NearChainProvider, Error> {
    let chain = NearChainReference::try_from(network.chain_id.clone()).map_err(|err| {
        Error::config_with(format!("invalid NEAR chain id '{}'", network.chain_id), err)
    })?;
    let rpc_url = resolve_optional_rpc(&network.chain_id, network.rpc.as_ref(), lookup)?;
    let mut relayers = Vec::with_capacity(network.relayers.len());
    for account in &network.relayers {
        let secret = named_secret(config, &account.signer, lookup)?;
        relayers.push(
            NearRelayer::from_secret_key(&account.account_id, secret).map_err(|err| {
                Error::config_with(
                    format!("signer '{}' is not a valid NEAR secret key", account.signer),
                    err,
                )
            })?,
        );
    }
    let mut provider = NearChainProvider::new(chain, relayers, rpc_url);
    if let Some(gas) = network.max_sponsored_gas {
        provider = provider.with_max_sponsored_gas(gas);
    }
    Ok(provider)
}

fn insert_scheme(
    map: &mut FacilitatorMap,
    network: &NearNetwork,
    name: &'static str,
    handler: NearExactFacilitator<NearChainProvider>,
) -> Result<(), Error> {
    let slug = SchemeSlug::new(network.chain_id.clone(), CompactString::from(name));
    map.insert(slug, Arc::new(handler))
}
