//! EIP-155 provider + exact facilitator wiring.

use std::str::FromStr;
use std::sync::Arc;

use alloy_network::EthereumWallet;
use alloy_primitives::Address;
use alloy_signer_local::PrivateKeySigner;
use compact_str::CompactString;
use r402_evm::Eip155ExactFacilitator;
use r402_evm::chain::{Eip155ChainProvider, Eip155ChainReference};
use r402_extensions::{
    BUILDER_CODE, BuilderCodeFacilitatorConfig, BuilderCodeFacilitatorExtension,
    ERC20_APPROVAL_GAS_SPONSORING_KEY,
};
use r402_facilitator::{InMemoryPendingSettlementStore, PendingSettlementStore, SettlementCache};
use r402_protocol::ExactScheme;
use r402_protocol::scheme::SchemeSlug;
use url::Url;

use super::{FacilitatorMap, scheme_not_enabled};
use crate::config::{
    BuilderCodeToml, Config, EvmNetwork, EvmSchemeConfig, RpcEndpoint, resolve_rpc,
};
use crate::error::Error;

/// Process-wide EVM exact construction state.
pub(super) struct Prepare {
    cache: SettlementCache,
    pending: Arc<dyn PendingSettlementStore>,
    settings: Settings,
}

struct Settings {
    clock_skew_secs: Option<u64>,
    factories: Vec<Address>,
    erc20: bool,
    builder_code: Option<BuilderCodeFacilitatorExtension>,
}

impl Prepare {
    pub(super) fn new(config: &Config) -> Result<Self, Error> {
        Ok(Self {
            cache: SettlementCache::new(),
            pending: Arc::new(InMemoryPendingSettlementStore::new()),
            settings: Settings::from_config(&config.scheme.evm)?,
        })
    }

    pub(super) fn register(
        &self,
        map: &mut FacilitatorMap,
        network: &EvmNetwork,
        config: &Config,
        lookup: &impl Fn(&str) -> Option<String>,
    ) -> Result<(), Error> {
        let endpoints = resolve_rpc(&network.chain_id, &network.rpc, lookup)?;
        let provider = Arc::new(provider(
            network,
            wallet(network, config, lookup)?,
            &endpoints,
        )?);
        for name in &network.schemes {
            match name.as_str() {
                ExactScheme::VALUE => self.register_exact(map, &provider, network)?,
                _ => return Err(scheme_not_enabled(name, &network.chain_id)),
            }
        }
        Ok(())
    }

    pub(super) fn finish(&self, map: &mut FacilitatorMap) {
        self.settings.apply_extensions(map);
    }

    fn register_exact(
        &self,
        map: &mut FacilitatorMap,
        provider: &Arc<Eip155ChainProvider>,
        network: &EvmNetwork,
    ) -> Result<(), Error> {
        let cache = self.cache.clone();
        let pending = Arc::clone(&self.pending);
        let mut facilitator =
            Eip155ExactFacilitator::with_settlement_cache(Arc::clone(provider), cache)
                .with_pending_store(pending)
                .with_eip6492_allowed_factories(self.settings.factories.clone());
        if let Some(secs) = self.settings.clock_skew_secs {
            facilitator = facilitator.with_clock_skew_tolerance(secs);
        }
        if self.settings.erc20 {
            facilitator = facilitator.with_erc20_approval_gas_sponsoring();
        }
        if let Some(extension) = self.settings.builder_code.clone() {
            facilitator = facilitator.with_builder_code(extension);
        }
        let slug = SchemeSlug::new(
            network.chain_id.clone(),
            CompactString::from(ExactScheme::VALUE),
        );
        map.insert(slug, Arc::new(facilitator))
    }
}

impl Settings {
    fn from_config(cfg: &EvmSchemeConfig) -> Result<Self, Error> {
        Ok(Self {
            clock_skew_secs: cfg.clock_skew_secs,
            factories: parse_factories(&cfg.eip6492_allowed_factories)?,
            erc20: cfg.erc20_approval_gas_sponsoring,
            builder_code: cfg
                .builder_code
                .as_ref()
                .map(builder_extension)
                .transpose()?,
        })
    }

    fn apply_extensions(&self, map: &mut FacilitatorMap) {
        if self.erc20 {
            map.push_extension(ERC20_APPROVAL_GAS_SPONSORING_KEY);
        }
        if self.builder_code.is_some() {
            map.push_extension(BUILDER_CODE);
        }
    }
}

fn provider(
    network: &EvmNetwork,
    wallet: EthereumWallet,
    endpoints: &[RpcEndpoint],
) -> Result<Eip155ChainProvider, Error> {
    let chain = Eip155ChainReference::try_from(network.chain_id.clone()).map_err(|err| {
        Error::config_with(
            format!("invalid EIP-155 chain id '{}'", network.chain_id),
            err,
        )
    })?;
    let pairs: Vec<(Url, Option<u32>)> = endpoints
        .iter()
        .map(|endpoint| (endpoint.url.clone(), endpoint.rate_limit))
        .collect();
    Eip155ChainProvider::new(
        chain,
        wallet,
        &pairs,
        network.eip1559,
        network.flashblocks,
        network.receipt_timeout_secs,
    )
    .map_err(|err| {
        Error::config_with(
            format!(
                "failed to build Eip155ChainProvider for '{}'",
                network.chain_id
            ),
            err,
        )
    })
}

fn wallet(
    network: &EvmNetwork,
    config: &Config,
    lookup: &impl Fn(&str) -> Option<String>,
) -> Result<EthereumWallet, Error> {
    let mut parsed = Vec::with_capacity(network.signers.len());
    for name in &network.signers {
        parsed.push(parse_evm_key(name, &named_secret(config, name, lookup)?)?);
    }
    let mut signers = parsed.into_iter();
    let Some(first) = signers.next() else {
        return Err(Error::config(format!(
            "[network.\"{}\"] `signers` must not be empty",
            network.chain_id
        )));
    };
    let mut wallet = EthereumWallet::from(first);
    for signer in signers {
        wallet.register_signer(signer);
    }
    Ok(wallet)
}

fn named_secret(
    config: &Config,
    name: &str,
    lookup: &impl Fn(&str) -> Option<String>,
) -> Result<String, Error> {
    let spec = config.signers.get(name).ok_or_else(|| {
        Error::config(format!(
            "[network.\"…\"] references unknown signer '{name}'"
        ))
    })?;
    spec.resolve(lookup).map_err(|err| match err {
        Error::Secret { context, source } => Error::Secret {
            context: format!("signer '{name}': {context}"),
            source,
        },
        other => other,
    })
}

fn parse_evm_key(name: &str, secret: &str) -> Result<PrivateKeySigner, Error> {
    PrivateKeySigner::from_str(secret).map_err(|err| {
        Error::config_with(
            format!("signer '{name}' is not a valid secp256k1 hex key"),
            err,
        )
    })
}

fn parse_factories(raw: &[String]) -> Result<Vec<Address>, Error> {
    raw.iter()
        .map(|value| {
            Address::from_str(value).map_err(|err| {
                Error::config_with(format!("invalid eip6492 factory address '{value}'"), err)
            })
        })
        .collect()
}

fn builder_extension(toml: &BuilderCodeToml) -> Result<BuilderCodeFacilitatorExtension, Error> {
    let config = BuilderCodeFacilitatorConfig {
        builder_code: toml.builder_code.as_deref().map(CompactString::from),
        service_code: toml.service_code.as_deref().map(CompactString::from),
    };
    BuilderCodeFacilitatorExtension::try_from_config(config)
        .map_err(|err| Error::config_with("invalid [scheme.evm] builder_code", err))
}
