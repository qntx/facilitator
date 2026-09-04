//! Solana provider + exact/upto facilitator wiring.
//!
//! Exact and upto share one [`SolanaChainProvider`] per network (CU limits
//! live on the network table). Upto uses process-wide
//! [`InMemoryChannelStorage`]; r402 0.19.1 does not expose a rent-cleanup
//! manager, so the channel index is lost on restart and Distributed-channel
//! rent is not reclaimed. v1 is a single replica.

use std::str::FromStr;
use std::sync::Arc;

use compact_str::CompactString;
use r402_facilitator::{DynFacilitator, PendingSettlementStore, SettlementCache};
use r402_protocol::scheme::SchemeSlug;
use r402_protocol::{ChainId, ExactScheme, UptoScheme};
use r402_svm::chain::{Address, SolanaChainProvider, SolanaChainReference};
use r402_svm::exact::facilitator::{SolanaExactFacilitator, SolanaExactFacilitatorConfig};
use r402_svm::upto::facilitator::{
    InMemoryChannelStorage, SolanaUptoFacilitator, SolanaUptoFacilitatorConfig, UptoChannelStorage,
};
use solana_keypair::Keypair;

use super::{FacilitatorMap, scheme_not_enabled};
use crate::config::{RpcEndpoint, SvmExactConfig, SvmNetwork, SvmSchemeConfig, SvmUptoConfig};
use crate::error::Error;

/// Process-wide SVM exact/upto construction state.
pub(super) struct Prepare {
    cache: SettlementCache,
    pending: Arc<dyn PendingSettlementStore>,
    storage: Arc<dyn UptoChannelStorage>,
    exact: SvmExactConfig,
    upto: SvmUptoConfig,
}

impl Prepare {
    pub(super) fn new(
        scheme: SvmSchemeConfig,
        cache: SettlementCache,
        pending: Arc<dyn PendingSettlementStore>,
    ) -> Self {
        Self {
            cache,
            pending,
            storage: Arc::new(InMemoryChannelStorage::new()),
            exact: scheme.exact,
            upto: scheme.upto,
        }
    }

    pub(super) async fn register(
        &self,
        map: &mut FacilitatorMap,
        network: &SvmNetwork,
        secret: &str,
        endpoints: Vec<RpcEndpoint>,
    ) -> Result<(), Error> {
        let keypair = parse_svm_key(&network.fee_payer, secret)?;
        let provider = Arc::new(provider(network, keypair, &endpoints).await?);
        for name in &network.schemes {
            match name.as_str() {
                ExactScheme::VALUE => self.register_exact(map, &provider, network)?,
                UptoScheme::VALUE => self.register_upto(map, &provider, network)?,
                _ => return Err(scheme_not_enabled(name, &network.chain_id)),
            }
        }
        Ok(())
    }

    fn register_exact(
        &self,
        map: &mut FacilitatorMap,
        provider: &Arc<SolanaChainProvider>,
        network: &SvmNetwork,
    ) -> Result<(), Error> {
        let config = exact_config(&self.exact, network.exact.as_ref(), &network.chain_id)?;
        let facilitator = SolanaExactFacilitator::with_settlement_cache(
            Arc::clone(provider),
            config,
            self.cache.clone(),
        )
        .with_pending_store(Arc::clone(&self.pending));
        insert_scheme(map, network, ExactScheme::VALUE, Arc::new(facilitator))
    }

    fn register_upto(
        &self,
        map: &mut FacilitatorMap,
        provider: &Arc<SolanaChainProvider>,
        network: &SvmNetwork,
    ) -> Result<(), Error> {
        // No settlement-cache constructor. `new` allocates private storage and
        // pending; override both so every SVM upto handler shares the process
        // stores. r402 0.19.1 has no rent-cleanup manager to spawn.
        let config = upto_config(&self.upto, network.upto.as_ref());
        let facilitator = SolanaUptoFacilitator::new(Arc::clone(provider), config)
            .with_storage(Arc::clone(&self.storage))
            .with_pending_store(Arc::clone(&self.pending));
        insert_scheme(map, network, UptoScheme::VALUE, Arc::new(facilitator))
    }
}

fn insert_scheme(
    map: &mut FacilitatorMap,
    network: &SvmNetwork,
    name: &'static str,
    handler: Arc<dyn DynFacilitator>,
) -> Result<(), Error> {
    let slug = SchemeSlug::new(network.chain_id.clone(), CompactString::from(name));
    map.insert(slug, handler)
}

async fn provider(
    network: &SvmNetwork,
    keypair: Keypair,
    endpoints: &[RpcEndpoint],
) -> Result<SolanaChainProvider, Error> {
    let chain = SolanaChainReference::try_from(network.chain_id.clone()).map_err(|err| {
        Error::config_with(
            format!("invalid Solana chain id '{}'", network.chain_id),
            err,
        )
    })?;
    let rpc_url = rpc_url(network, endpoints)?;
    SolanaChainProvider::new(
        keypair,
        rpc_url,
        network.pubsub.clone(),
        chain,
        network.max_compute_unit_limit,
        network.max_compute_unit_price,
    )
    .await
    .map_err(|err| {
        Error::config_with(
            format!(
                "failed to build SolanaChainProvider for '{}'",
                network.chain_id
            ),
            err,
        )
    })
}

fn rpc_url(network: &SvmNetwork, endpoints: &[RpcEndpoint]) -> Result<String, Error> {
    endpoints
        .first()
        .map(|endpoint| endpoint.url.as_str().to_owned())
        .ok_or_else(|| Error::config(format!("[network.\"{}\"] has no RPC URL", network.chain_id)))
}

fn parse_svm_key(name: &str, secret: &str) -> Result<Keypair, Error> {
    Keypair::try_from_base58_string(secret).map_err(|err| {
        Error::config_with(
            format!("signer '{name}' is not a valid base58 Solana keypair"),
            err,
        )
    })
}

fn exact_config(
    global: &SvmExactConfig,
    overlay: Option<&SvmExactConfig>,
    chain_id: &ChainId,
) -> Result<SolanaExactFacilitatorConfig, Error> {
    let merged = overlay_exact(global, overlay);
    reject_path2(chain_id, &merged)?;
    apply_exact(&merged)
}

fn reject_path2(chain_id: &ChainId, merged: &SvmExactConfig) -> Result<(), Error> {
    if merged.enable_smart_wallet_verification == Some(true) {
        return Err(Error::config(format!(
            "enable_smart_wallet_verification = true on {chain_id} is not supported in this build"
        )));
    }
    Ok(())
}

fn overlay_exact(base: &SvmExactConfig, overlay: Option<&SvmExactConfig>) -> SvmExactConfig {
    let Some(over) = overlay else {
        return base.clone();
    };
    SvmExactConfig {
        allow_additional_instructions: over
            .allow_additional_instructions
            .or(base.allow_additional_instructions),
        max_instruction_count: over.max_instruction_count.or(base.max_instruction_count),
        allowed_program_ids: over
            .allowed_program_ids
            .clone()
            .or_else(|| base.allowed_program_ids.clone()),
        blocked_program_ids: over
            .blocked_program_ids
            .clone()
            .or_else(|| base.blocked_program_ids.clone()),
        enable_smart_wallet_verification: over
            .enable_smart_wallet_verification
            .or(base.enable_smart_wallet_verification),
        smart_wallet_max_compute_units: over
            .smart_wallet_max_compute_units
            .or(base.smart_wallet_max_compute_units),
        smart_wallet_max_priority_fee_micro_lamports: over
            .smart_wallet_max_priority_fee_micro_lamports
            .or(base.smart_wallet_max_priority_fee_micro_lamports),
        smart_wallet_allowed_programs: over
            .smart_wallet_allowed_programs
            .clone()
            .or_else(|| base.smart_wallet_allowed_programs.clone()),
    }
}

fn apply_exact(merged: &SvmExactConfig) -> Result<SolanaExactFacilitatorConfig, Error> {
    let mut config = SolanaExactFacilitatorConfig::default();
    if let Some(value) = merged.allow_additional_instructions {
        config.allow_additional_instructions = value;
    }
    if let Some(value) = merged.max_instruction_count {
        config.max_instruction_count = value;
    }
    if let Some(ids) = &merged.allowed_program_ids {
        config.allowed_program_ids = parse_program_ids(ids)?;
    }
    if let Some(ids) = &merged.blocked_program_ids {
        config.blocked_program_ids = parse_program_ids(ids)?;
    }
    config.enable_smart_wallet_verification = false;
    config.smart_wallet_max_compute_units = merged.smart_wallet_max_compute_units;
    config.smart_wallet_max_priority_fee_micro_lamports =
        merged.smart_wallet_max_priority_fee_micro_lamports;
    if let Some(programs) = &merged.smart_wallet_allowed_programs {
        config.smart_wallet_allowed_programs = Some(parse_program_ids(programs)?);
    }
    Ok(config)
}

fn parse_program_ids(raw: &[String]) -> Result<Vec<Address>, Error> {
    raw.iter()
        .map(|value| {
            Address::from_str(value)
                .map_err(|err| Error::config(format!("invalid Solana program id '{value}': {err}")))
        })
        .collect()
}

fn upto_config(
    global: &SvmUptoConfig,
    overlay: Option<&SvmUptoConfig>,
) -> SolanaUptoFacilitatorConfig {
    apply_upto(&overlay_upto(global, overlay))
}

fn overlay_upto(base: &SvmUptoConfig, overlay: Option<&SvmUptoConfig>) -> SvmUptoConfig {
    let Some(over) = overlay else {
        return *base;
    };
    SvmUptoConfig {
        max_channel_lifetime_secs: over
            .max_channel_lifetime_secs
            .or(base.max_channel_lifetime_secs),
        max_priority_fee_micro_lamports: over
            .max_priority_fee_micro_lamports
            .or(base.max_priority_fee_micro_lamports),
        max_compute_units: over.max_compute_units.or(base.max_compute_units),
        max_required_signatures: over
            .max_required_signatures
            .or(base.max_required_signatures),
        compute_unit_price_micro_lamports: over
            .compute_unit_price_micro_lamports
            .or(base.compute_unit_price_micro_lamports),
        settle_compute_unit_limit: over
            .settle_compute_unit_limit
            .or(base.settle_compute_unit_limit),
    }
}

fn apply_upto(merged: &SvmUptoConfig) -> SolanaUptoFacilitatorConfig {
    let mut config = SolanaUptoFacilitatorConfig::default();
    if let Some(value) = merged.max_channel_lifetime_secs {
        config.max_channel_lifetime_secs = value;
    }
    if let Some(value) = merged.max_priority_fee_micro_lamports {
        config.max_priority_fee_micro_lamports = value;
    }
    if let Some(value) = merged.max_compute_units {
        config.max_compute_units = value;
    }
    if let Some(value) = merged.max_required_signatures {
        config.max_required_signatures = Some(value);
    }
    if let Some(value) = merged.compute_unit_price_micro_lamports {
        config.compute_unit_price_micro_lamports = value;
    }
    if let Some(value) = merged.settle_compute_unit_limit {
        config.settle_compute_unit_limit = value;
    }
    config
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "unit tests"
)]
mod tests {
    use super::*;

    #[test]
    fn overlay_upto_prefers_network_keys_and_keeps_global() {
        let base = SvmUptoConfig {
            max_channel_lifetime_secs: Some(3_600),
            ..SvmUptoConfig::default()
        };
        let over = SvmUptoConfig {
            max_compute_units: Some(300_000),
            ..SvmUptoConfig::default()
        };
        let merged = overlay_upto(&base, Some(&over));
        assert_eq!(
            merged.max_channel_lifetime_secs,
            Some(3_600),
            "global lifetime"
        );
        assert_eq!(
            merged.max_compute_units,
            Some(300_000),
            "network CU overlay"
        );
    }

    #[test]
    fn apply_upto_omitted_keeps_sdk_defaults() {
        let config = apply_upto(&SvmUptoConfig::default());
        let default = SolanaUptoFacilitatorConfig::default();
        assert_eq!(
            config.max_channel_lifetime_secs, default.max_channel_lifetime_secs,
            "lifetime"
        );
        assert_eq!(
            config.max_priority_fee_micro_lamports, default.max_priority_fee_micro_lamports,
            "priority fee"
        );
        assert_eq!(config.max_compute_units, default.max_compute_units, "CU");
        assert_eq!(
            config.max_required_signatures, default.max_required_signatures,
            "signatures"
        );
        assert_eq!(
            config.compute_unit_price_micro_lamports, default.compute_unit_price_micro_lamports,
            "settle price"
        );
        assert_eq!(
            config.settle_compute_unit_limit, default.settle_compute_unit_limit,
            "settle CU"
        );
    }
}
