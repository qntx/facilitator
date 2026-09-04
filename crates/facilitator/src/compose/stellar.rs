//! Stellar provider + exact facilitator wiring.

use std::sync::Arc;

use compact_str::CompactString;
use r402_facilitator::SettlementCache;
use r402_protocol::ExactScheme;
use r402_protocol::scheme::SchemeSlug;
use r402_stellar::chain::StellarChainReference;
use r402_stellar::{StellarChainProvider, StellarExactFacilitator, StellarSigner};

use super::{FacilitatorMap, named_secret, scheme_not_enabled};
use crate::config::{Config, StellarNetwork, resolve_optional_rpc};
use crate::error::Error;

/// Register Stellar exact for `network`.
pub(super) fn register(
    map: &mut FacilitatorMap,
    network: &StellarNetwork,
    config: &Config,
    lookup: &impl Fn(&str) -> Option<String>,
    cache: &SettlementCache,
) -> Result<(), Error> {
    let provider = provider(network, config, lookup)?;
    let max_fee = provider.max_transaction_fee_stroops();
    for name in &network.schemes {
        match name.as_str() {
            ExactScheme::VALUE => {
                let facilitator = StellarExactFacilitator::with_settlement_cache(
                    provider.clone(),
                    max_fee,
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
    network: &StellarNetwork,
    config: &Config,
    lookup: &impl Fn(&str) -> Option<String>,
) -> Result<StellarChainProvider, Error> {
    let chain = StellarChainReference::try_from(network.chain_id.clone()).map_err(|err| {
        Error::config_with(
            format!("invalid Stellar chain id '{}'", network.chain_id),
            err,
        )
    })?;
    let rpc_url = resolve_optional_rpc(&network.chain_id, network.rpc.as_ref(), lookup)?;
    let mut signers = Vec::with_capacity(network.signers.len());
    for name in &network.signers {
        signers.push(stellar_signer(config, name, lookup)?);
    }
    let mut provider =
        StellarChainProvider::new(chain, signers, rpc_url.as_deref()).map_err(|err| {
            Error::config_with(
                format!(
                    "failed to build StellarChainProvider for '{}'",
                    network.chain_id
                ),
                err,
            )
        })?;
    if let Some(url) = &network.horizon_url {
        provider = provider.with_horizon_url(url.as_str());
    }
    if let Some(fee) = network.max_transaction_fee_stroops {
        provider = provider.with_max_transaction_fee_stroops(fee);
    }
    if let Some(name) = &network.fee_bump {
        provider = provider.with_fee_bump_signer(stellar_signer(config, name, lookup)?);
    }
    Ok(provider)
}

fn stellar_signer(
    config: &Config,
    name: &str,
    lookup: &impl Fn(&str) -> Option<String>,
) -> Result<StellarSigner, Error> {
    let secret = named_secret(config, name, lookup)?;
    StellarSigner::from_secret(&secret).map_err(|err| {
        Error::config_with(
            format!("signer '{name}' is not a valid Stellar secret key"),
            err,
        )
    })
}

fn insert_scheme(
    map: &mut FacilitatorMap,
    network: &StellarNetwork,
    name: &'static str,
    handler: StellarExactFacilitator<StellarChainProvider>,
) -> Result<(), Error> {
    let slug = SchemeSlug::new(network.chain_id.clone(), CompactString::from(name));
    map.insert(slug, Arc::new(handler))
}
