//! XRPL provider + exact facilitator wiring.

use std::sync::Arc;

use compact_str::CompactString;
use r402_protocol::ExactScheme;
use r402_protocol::scheme::SchemeSlug;
use r402_xrpl::chain::XrplChainReference;
use r402_xrpl::exact::facilitator::XrplSettlementCache;
use r402_xrpl::{XrplChainProvider, XrplExactFacilitator};

use super::{FacilitatorMap, scheme_not_enabled};
use crate::config::{XrplNetwork, resolve_optional_rpc};
use crate::error::Error;

/// Register XRPL exact for `network`.
pub(super) fn register(
    map: &mut FacilitatorMap,
    network: &XrplNetwork,
    lookup: &impl Fn(&str) -> Option<String>,
    cache: &XrplSettlementCache,
) -> Result<(), Error> {
    let provider = provider(network, lookup)?;
    let max_fee_drops = provider.max_fee_drops();
    for name in &network.schemes {
        match name.as_str() {
            ExactScheme::VALUE => {
                let facilitator = XrplExactFacilitator::with_settlement_cache(
                    provider.clone(),
                    max_fee_drops,
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
    network: &XrplNetwork,
    lookup: &impl Fn(&str) -> Option<String>,
) -> Result<XrplChainProvider, Error> {
    let chain = XrplChainReference::try_from(network.chain_id.clone()).map_err(|err| {
        Error::config_with(format!("invalid XRPL chain id '{}'", network.chain_id), err)
    })?;
    let rpc_url = resolve_optional_rpc(&network.chain_id, network.rpc.as_ref(), lookup)?;
    let mut provider = XrplChainProvider::new(chain, rpc_url).map_err(|err| {
        Error::config_with(
            format!(
                "failed to build XrplChainProvider for '{}'",
                network.chain_id
            ),
            err,
        )
    })?;
    if let Some(drops) = network.max_fee_drops {
        provider = provider.with_max_fee_drops(drops);
    }
    Ok(provider)
}

fn insert_scheme(
    map: &mut FacilitatorMap,
    network: &XrplNetwork,
    name: &'static str,
    handler: XrplExactFacilitator<XrplChainProvider>,
) -> Result<(), Error> {
    let slug = SchemeSlug::new(network.chain_id.clone(), CompactString::from(name));
    map.insert(slug, Arc::new(handler))
}
