//! TVM provider + exact facilitator wiring.

use std::sync::Arc;

use compact_str::CompactString;
use r402_facilitator::SettlementCache;
use r402_protocol::ExactScheme;
use r402_protocol::scheme::SchemeSlug;
use r402_tvm::chain::TvmChainReference;
use r402_tvm::exact::facilitator::TvmExactFacilitatorConfig;
use r402_tvm::{HighloadV3Config, TvmChainProvider, TvmExactFacilitator};

use super::{FacilitatorMap, named_secret, scheme_not_enabled};
use crate::config::{Config, TvmNetwork, resolve_api_key};
use crate::error::Error;

/// Register TVM exact for `network`.
pub(super) fn register(
    map: &mut FacilitatorMap,
    network: &TvmNetwork,
    config: &Config,
    lookup: &impl Fn(&str) -> Option<String>,
    cache: &SettlementCache,
) -> Result<(), Error> {
    let provider = provider(network, config, lookup)?;
    let cfg = TvmExactFacilitatorConfig {
        batch_flush_interval_seconds: network.batch_flush_interval_seconds,
        batch_flush_size: network.batch_flush_size,
        confirmation_timeout_seconds: network.confirmation_timeout_seconds,
    };
    for name in &network.schemes {
        match name.as_str() {
            ExactScheme::VALUE => {
                let facilitator = TvmExactFacilitator::with_settlement_cache(
                    provider.clone(),
                    cfg,
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
    network: &TvmNetwork,
    config: &Config,
    lookup: &impl Fn(&str) -> Option<String>,
) -> Result<TvmChainProvider, Error> {
    let chain = TvmChainReference::try_from(network.chain_id.clone()).map_err(|err| {
        Error::config_with(format!("invalid TVM chain id '{}'", network.chain_id), err)
    })?;
    let secret = named_secret(config, &network.signer, lookup)?;
    let mut hl = HighloadV3Config::from_private_key_str(&secret).map_err(|err| {
        Error::config_with(
            format!(
                "signer '{}' is not a valid TVM Highload V3 private key",
                network.signer
            ),
            err,
        )
    })?;
    if let Some(url) = &network.provider_base_url {
        hl.provider_base_url = Some(url.as_str().to_owned());
    }
    hl.api_key = resolve_api_key(&network.chain_id, network.api_key_env.as_deref(), lookup)?;
    if let Some(id) = network.subwallet_id {
        hl.subwallet_id = id;
    }
    if let Some(timeout) = network.timeout {
        hl.timeout = timeout;
    }
    if let Some(workchain) = network.workchain {
        hl.workchain = workchain;
    }
    TvmChainProvider::new(chain, hl).map_err(|err| {
        Error::config_with(
            format!(
                "failed to build TvmChainProvider for '{}'",
                network.chain_id
            ),
            err,
        )
    })
}

fn insert_scheme(
    map: &mut FacilitatorMap,
    network: &TvmNetwork,
    name: &'static str,
    handler: TvmExactFacilitator,
) -> Result<(), Error> {
    let slug = SchemeSlug::new(network.chain_id.clone(), CompactString::from(name));
    map.insert(slug, Arc::new(handler))
}
