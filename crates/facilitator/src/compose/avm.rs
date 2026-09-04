//! Algorand provider + exact facilitator wiring.

use std::sync::Arc;

use compact_str::CompactString;
use r402_avm::chain::AlgorandChainReference;
use r402_avm::{
    AlgorandChainProvider, AlgorandExactFacilitator, AlgorandSigner, DEFAULT_WAIT_ROUNDS,
};
use r402_facilitator::SettlementCache;
use r402_protocol::ExactScheme;
use r402_protocol::scheme::SchemeSlug;

use super::{FacilitatorMap, named_secret, scheme_not_enabled};
use crate::config::{AvmNetwork, Config, resolve_algod_token};
use crate::error::Error;

/// Register Algorand exact for `network`.
pub(super) fn register(
    map: &mut FacilitatorMap,
    network: &AvmNetwork,
    config: &Config,
    lookup: &impl Fn(&str) -> Option<String>,
    cache: &SettlementCache,
) -> Result<(), Error> {
    let provider = provider(network, config, lookup)?;
    let wait_rounds = network.wait_rounds.unwrap_or(DEFAULT_WAIT_ROUNDS);
    for name in &network.schemes {
        match name.as_str() {
            ExactScheme::VALUE => {
                let facilitator = AlgorandExactFacilitator::with_settlement_cache(
                    provider.clone(),
                    wait_rounds,
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
    network: &AvmNetwork,
    config: &Config,
    lookup: &impl Fn(&str) -> Option<String>,
) -> Result<AlgorandChainProvider, Error> {
    let chain = AlgorandChainReference::try_from(network.chain_id.clone()).map_err(|err| {
        Error::config_with(
            format!("invalid Algorand chain id '{}'", network.chain_id),
            err,
        )
    })?;
    let mut signers = Vec::with_capacity(network.signers.len());
    for name in &network.signers {
        let secret = named_secret(config, name, lookup)?;
        signers.push(AlgorandSigner::from_base64_secret(secret).map_err(|err| {
            Error::config_with(
                format!("signer '{name}' is not a valid Algorand base64 seed"),
                err,
            )
        })?);
    }
    let algod_url = network
        .algod_url
        .as_ref()
        .map(|url| url.as_str().to_owned());
    let algod_token = resolve_algod_token(
        &network.chain_id,
        network.algod_token_env.as_deref(),
        lookup,
    )?;
    Ok(AlgorandChainProvider::new(
        chain,
        signers,
        algod_url,
        algod_token,
    ))
}

fn insert_scheme(
    map: &mut FacilitatorMap,
    network: &AvmNetwork,
    name: &'static str,
    handler: AlgorandExactFacilitator<AlgorandChainProvider>,
) -> Result<(), Error> {
    let slug = SchemeSlug::new(network.chain_id.clone(), CompactString::from(name));
    map.insert(slug, Arc::new(handler))
}
