//! Tron provider + exact facilitator wiring.

use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

use alloy_signer_local::PrivateKeySigner;
use compact_str::CompactString;
use r402_facilitator::SettlementCache;
use r402_protocol::ChainId;
use r402_protocol::ExactScheme;
use r402_protocol::scheme::SchemeSlug;
use r402_tron::TronExactFacilitator;
use r402_tron::chain::{TronChainProvider, TronChainProviderConfig, TronChainReference};
use url::Url;

use super::{FacilitatorMap, named_secret, scheme_not_enabled};
use crate::config::{Config, RpcEndpoint, TronNetwork, resolve_rpc};
use crate::error::Error;

/// Default `fee_limit` in SUN when the network table omits it.
const DEFAULT_FEE_LIMIT_SUN: u64 = 100_000_000;

/// Default confirmation wait (Tron blocks are ~3s).
const DEFAULT_CONFIRMATION_TIMEOUT: Duration = Duration::from_secs(30);

/// Default poll interval for `gettransactioninfobyid`.
const DEFAULT_CONFIRMATION_POLL_INTERVAL: Duration = Duration::from_secs(1);

/// Register Tron exact for `network`.
///
/// [`TronChainProvider::new`] does not dial; settlement talks to `TronGrid` later.
pub(super) fn register(
    map: &mut FacilitatorMap,
    network: &TronNetwork,
    config: &Config,
    lookup: &impl Fn(&str) -> Option<String>,
    cache: &SettlementCache,
) -> Result<(), Error> {
    for name in &network.schemes {
        if name.as_str() != ExactScheme::VALUE {
            return Err(scheme_not_enabled(name, &network.chain_id));
        }
    }
    let facilitator = TronExactFacilitator::with_settlement_cache(
        provider(network, config, lookup)?,
        cache.clone(),
    );
    insert_scheme(map, network, ExactScheme::VALUE, facilitator)
}

fn provider(
    network: &TronNetwork,
    config: &Config,
    lookup: &impl Fn(&str) -> Option<String>,
) -> Result<TronChainProvider, Error> {
    let chain = TronChainReference::try_from(network.chain_id.clone()).map_err(|err| {
        Error::config_with(format!("invalid Tron chain id '{}'", network.chain_id), err)
    })?;
    let endpoints = resolve_rpc(&network.chain_id, &network.rpc, lookup)?;
    let base_url = tron_base_url(&network.chain_id, &endpoints)?;
    let secret = named_secret(config, &network.signer, lookup)?;
    let signer = PrivateKeySigner::from_str(&secret).map_err(|err| {
        Error::config_with(
            format!(
                "signer '{}' is not a valid secp256k1 hex key",
                network.signer
            ),
            err,
        )
    })?;
    Ok(TronChainProvider::new(TronChainProviderConfig {
        chain_reference: chain,
        base_url,
        signer,
        fee_limit: network.fee_limit.unwrap_or(DEFAULT_FEE_LIMIT_SUN),
        confirmation_timeout: network
            .confirmation_timeout
            .unwrap_or(DEFAULT_CONFIRMATION_TIMEOUT),
        confirmation_poll_interval: network
            .confirmation_poll_interval
            .unwrap_or(DEFAULT_CONFIRMATION_POLL_INTERVAL),
    }))
}

fn tron_base_url(chain_id: &ChainId, endpoints: &[RpcEndpoint]) -> Result<Url, Error> {
    let Some(endpoint) = endpoints.first() else {
        return Err(Error::config(format!(
            "[network.\"{chain_id}\"] has no RPC URL"
        )));
    };
    Ok(endpoint.url.clone())
}

fn insert_scheme(
    map: &mut FacilitatorMap,
    network: &TronNetwork,
    name: &'static str,
    handler: TronExactFacilitator,
) -> Result<(), Error> {
    let slug = SchemeSlug::new(network.chain_id.clone(), CompactString::from(name));
    map.insert(slug, Arc::new(handler))
}
