//! Solana provider + exact facilitator wiring.

use std::str::FromStr;
use std::sync::Arc;

use compact_str::CompactString;
use r402_facilitator::{DynFacilitator, PendingSettlementStore, SettlementCache};
use r402_protocol::scheme::SchemeSlug;
use r402_protocol::{ChainId, ExactScheme};
use r402_svm::chain::{Address, SolanaChainProvider, SolanaChainReference};
use r402_svm::exact::facilitator::{SolanaExactFacilitator, SolanaExactFacilitatorConfig};
use solana_keypair::Keypair;

use super::{FacilitatorMap, scheme_not_enabled};
use crate::config::{RpcEndpoint, SvmExactConfig, SvmNetwork};
use crate::error::Error;

/// Process-wide SVM exact construction state.
pub(super) struct Prepare {
    cache: SettlementCache,
    pending: Arc<dyn PendingSettlementStore>,
    exact: SvmExactConfig,
}

impl Prepare {
    pub(super) fn new(
        exact: SvmExactConfig,
        cache: SettlementCache,
        pending: Arc<dyn PendingSettlementStore>,
    ) -> Self {
        Self {
            cache,
            pending,
            exact,
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
