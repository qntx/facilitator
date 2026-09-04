//! Concordium provider + exact facilitator wiring.

use std::sync::Arc;

use compact_str::CompactString;
use r402_concordium::chain::ConcordiumChainReference;
use r402_concordium::{
    ConcordiumChainProvider, ConcordiumExactFacilitator, ConcordiumGrpc, ConcordiumSigner,
};
use r402_facilitator::SettlementCache;
use r402_protocol::ExactScheme;
use r402_protocol::scheme::SchemeSlug;

use super::{FacilitatorMap, named_secret, scheme_not_enabled};
use crate::config::{ConcordiumNetwork, Config, resolve_optional_grpc};
use crate::error::Error;

/// Register Concordium exact for `network`.
///
/// [`ConcordiumChainProvider::connect`] dials gRPC. Tests that must not
/// talk to a node should parse only.
pub(super) async fn register(
    map: &mut FacilitatorMap,
    network: &ConcordiumNetwork,
    config: &Config,
    lookup: &(impl Fn(&str) -> Option<String> + Send + Sync),
    cache: &SettlementCache,
) -> Result<(), Error> {
    for name in &network.schemes {
        if name.as_str() != ExactScheme::VALUE {
            return Err(scheme_not_enabled(name, &network.chain_id));
        }
    }
    let provider = provider(network, config, lookup).await?;
    let mut facilitator = ConcordiumExactFacilitator::try_new(provider)
        .map_err(|err| {
            Error::config_with(
                format!(
                    "failed to construct ConcordiumExactFacilitator for '{}'",
                    network.chain_id
                ),
                err,
            )
        })?
        .with_settlement_cache(cache.clone());
    facilitator = facilitator.with_require_finalization(network.require_finalization);
    if let Some(timeout) = network.finalization_timeout {
        facilitator = facilitator.with_finalization_timeout(timeout);
    }
    if let Some(offset) = network.max_expiry_offset_seconds {
        facilitator = facilitator.with_max_expiry_offset_seconds(offset);
    }
    insert_scheme(map, network, ExactScheme::VALUE, facilitator)
}

async fn provider(
    network: &ConcordiumNetwork,
    config: &Config,
    lookup: &(impl Fn(&str) -> Option<String> + Send + Sync),
) -> Result<ConcordiumChainProvider, Error> {
    let chain = ConcordiumChainReference::try_from(network.chain_id.clone()).map_err(|err| {
        Error::config_with(
            format!("invalid Concordium chain id '{}'", network.chain_id),
            err,
        )
    })?;
    let grpc_url = resolve_optional_grpc(&network.chain_id, network.grpc.as_ref(), lookup)?;
    let mut signers = Vec::with_capacity(network.signers.len());
    for account in &network.signers {
        let secret = named_secret(config, &account.signer, lookup)?;
        signers.push(
            ConcordiumSigner::from_secret(&account.address, secret).map_err(|err| {
                Error::config_with(
                    format!(
                        "signer '{}' is not a valid Concordium address+seed",
                        account.signer
                    ),
                    err,
                )
            })?,
        );
    }
    ConcordiumChainProvider::connect(chain, signers, grpc_url)
        .await
        .map_err(|err| {
            Error::config_with(
                format!(
                    "failed to connect Concordium gRPC for '{}'",
                    network.chain_id
                ),
                err,
            )
        })
}

fn insert_scheme(
    map: &mut FacilitatorMap,
    network: &ConcordiumNetwork,
    name: &'static str,
    handler: ConcordiumExactFacilitator<ConcordiumGrpc>,
) -> Result<(), Error> {
    let slug = SchemeSlug::new(network.chain_id.clone(), CompactString::from(name));
    map.insert(slug, Arc::new(handler))
}
