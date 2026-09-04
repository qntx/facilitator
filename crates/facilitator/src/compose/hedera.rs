//! Hedera provider + exact facilitator wiring.

use std::sync::Arc;

use compact_str::CompactString;
use r402_facilitator::SettlementCache;
use r402_hedera::chain::HederaChainReference;
use r402_hedera::{AliasPolicy, HederaChainProvider, HederaExactFacilitator, HederaFeePayer};
use r402_protocol::ExactScheme;
use r402_protocol::scheme::SchemeSlug;

use super::{FacilitatorMap, named_secret, scheme_not_enabled};
use crate::config::{Config, HederaAliasPolicy, HederaNetwork};
use crate::error::Error;

/// Register Hedera exact for `network`.
pub(super) fn register(
    map: &mut FacilitatorMap,
    network: &HederaNetwork,
    config: &Config,
    lookup: &impl Fn(&str) -> Option<String>,
    cache: &SettlementCache,
) -> Result<(), Error> {
    let provider = provider(network, config, lookup)?;
    let alias_policy = alias_policy(network.alias_policy);
    for name in &network.schemes {
        match name.as_str() {
            ExactScheme::VALUE => {
                let facilitator = HederaExactFacilitator::with_settlement_cache(
                    provider.clone(),
                    alias_policy,
                    cache.clone(),
                );
                insert_scheme(map, network, ExactScheme::VALUE, facilitator)?;
            }
            _ => return Err(scheme_not_enabled(name, &network.chain_id)),
        }
    }
    Ok(())
}

const fn alias_policy(policy: HederaAliasPolicy) -> AliasPolicy {
    match policy {
        HederaAliasPolicy::Reject => AliasPolicy::Reject,
        HederaAliasPolicy::Allow => AliasPolicy::Allow,
    }
}

fn provider(
    network: &HederaNetwork,
    config: &Config,
    lookup: &impl Fn(&str) -> Option<String>,
) -> Result<HederaChainProvider, Error> {
    let chain = HederaChainReference::try_from(network.chain_id.clone()).map_err(|err| {
        Error::config_with(
            format!("invalid Hedera chain id '{}'", network.chain_id),
            err,
        )
    })?;
    let mut fee_payers = Vec::with_capacity(network.fee_payers.len());
    for account in &network.fee_payers {
        let secret = named_secret(config, &account.signer, lookup)?;
        fee_payers.push(
            HederaFeePayer::from_secret(&account.account_id, secret).map_err(|err| {
                Error::config_with(
                    format!(
                        "signer '{}' is not a valid Hedera private key",
                        account.signer
                    ),
                    err,
                )
            })?,
        );
    }
    let mut provider = HederaChainProvider::new(chain, fee_payers);
    if let Some(url) = &network.mirror_url {
        provider = provider.with_mirror_url(url.as_str());
    }
    if let Some(url) = &network.node_url {
        provider = provider.with_node_url(url.as_str());
    }
    Ok(provider)
}

fn insert_scheme(
    map: &mut FacilitatorMap,
    network: &HederaNetwork,
    name: &'static str,
    handler: HederaExactFacilitator<HederaChainProvider>,
) -> Result<(), Error> {
    let slug = SchemeSlug::new(network.chain_id.clone(), CompactString::from(name));
    map.insert(slug, Arc::new(handler))
}
