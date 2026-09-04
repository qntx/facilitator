//! Per-network tables keyed by CAIP-2 id.

use r402_protocol::{AuthCaptureScheme, BatchSettlementScheme, ChainId, ExactScheme, UptoScheme};
use serde::Deserialize;
use url::Url;

use super::family::HostableFamily;
use super::scheme::{SvmExactConfig, SvmUptoConfig};
use crate::error::Error;

/// Default EVM receipt wait; must finish inside `http.settle_timeout`.
const DEFAULT_RECEIPT_TIMEOUT_SECS: u64 = 20;

/// Default SVM compute-unit limit (`SolanaChainProvider::new`).
const DEFAULT_SVM_CU_LIMIT: u32 = 200_000;

/// One configured network after schema validation.
#[derive(Debug, Clone)]
#[allow(
    clippy::large_enum_variant,
    reason = "config value, not a hot-path enum"
)]
pub enum Network {
    /// EIP-155 network.
    Evm(EvmNetwork),
    /// Solana network.
    Svm(SvmNetwork),
}

impl Network {
    /// CAIP-2 identifier.
    #[must_use]
    pub const fn chain_id(&self) -> &ChainId {
        match self {
            Self::Evm(net) => &net.chain_id,
            Self::Svm(net) => &net.chain_id,
        }
    }

    /// Scheme names listed on this network.
    #[must_use]
    pub fn schemes(&self) -> &[String] {
        match self {
            Self::Evm(net) => &net.schemes,
            Self::Svm(net) => &net.schemes,
        }
    }

    /// Signer names referenced by this network.
    #[must_use]
    pub fn signer_names(&self) -> &[String] {
        match self {
            Self::Evm(net) => &net.signers,
            Self::Svm(net) => std::slice::from_ref(&net.fee_payer),
        }
    }
}

/// Parsed EIP-155 `[network."<caip2>"]`.
#[derive(Debug, Clone)]
pub struct EvmNetwork {
    /// CAIP-2 id (`eip155:<u64>`).
    pub chain_id: ChainId,
    /// RPC endpoints or an env name holding one URL.
    pub rpc: RpcConfig,
    /// Named `[signer.*]` references.
    pub signers: Vec<String>,
    /// Scheme names (`exact`, `upto`, …).
    pub schemes: Vec<String>,
    /// EIP-1559 gas pricing.
    pub eip1559: bool,
    /// Flashblocks hint.
    pub flashblocks: bool,
    /// Receipt wait in seconds.
    pub receipt_timeout_secs: u64,
}

/// Parsed Solana `[network."<caip2>"]`.
#[derive(Debug, Clone)]
pub struct SvmNetwork {
    /// CAIP-2 id.
    pub chain_id: ChainId,
    /// RPC URL or env name.
    pub rpc: RpcConfig,
    /// Optional pubsub URL.
    pub pubsub: Option<String>,
    /// Named fee-payer signer.
    pub fee_payer: String,
    /// Scheme names (`exact`, `upto`).
    pub schemes: Vec<String>,
    /// Compute-unit limit for the shared provider.
    pub max_compute_unit_limit: u32,
    /// Optional compute-unit price; `None` uses the SDK default.
    pub max_compute_unit_price: Option<u64>,
    /// Per-network exact override.
    pub exact: Option<SvmExactConfig>,
    /// Per-network upto override.
    pub upto: Option<SvmUptoConfig>,
}

/// RPC source: literal endpoints or one env var.
#[derive(Debug, Clone)]
pub enum RpcConfig {
    /// URLs from TOML.
    Literal(Vec<RpcEndpoint>),
    /// Environment variable whose value is one `http`/`https` URL.
    Env(String),
}

/// One HTTP RPC endpoint.
#[derive(Debug, Clone)]
pub struct RpcEndpoint {
    /// RPC URL.
    pub url: Url,
    /// Optional requests-per-second limit.
    pub rate_limit: Option<u32>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawEvmNetwork {
    /// Literal RPC list.
    #[serde(default)]
    rpc: Option<Vec<RawEvmRpc>>,
    /// Env name for a single RPC URL.
    #[serde(default)]
    rpc_env: Option<String>,
    /// Named signers.
    signers: Vec<String>,
    /// Scheme names.
    schemes: Vec<String>,
    /// EIP-1559 (default true).
    #[serde(default)]
    eip1559: Option<bool>,
    /// Flashblocks (default false).
    #[serde(default)]
    flashblocks: Option<bool>,
    /// Receipt timeout (default 20).
    #[serde(default)]
    receipt_timeout_secs: Option<u64>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawEvmRpcEndpoint {
    /// HTTP URL.
    http: String,
    /// Optional rate limit.
    #[serde(default)]
    rate_limit: Option<u32>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum RawEvmRpc {
    /// Bare URL string.
    Url(String),
    /// `{ http, rate_limit }`.
    Endpoint(RawEvmRpcEndpoint),
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawSvmNetwork {
    /// Literal RPC URL.
    #[serde(default)]
    rpc: Option<String>,
    /// Env name for the RPC URL.
    #[serde(default)]
    rpc_env: Option<String>,
    /// Optional pubsub URL.
    #[serde(default)]
    pubsub: Option<String>,
    /// Named fee-payer signer.
    fee_payer: String,
    /// Scheme names.
    schemes: Vec<String>,
    /// Provider CU limit.
    #[serde(default)]
    max_compute_unit_limit: Option<u32>,
    /// Provider CU price.
    #[serde(default)]
    max_compute_unit_price: Option<u64>,
    /// Per-network exact override.
    #[serde(default)]
    exact: Option<SvmExactConfig>,
    /// Per-network upto override.
    #[serde(default)]
    upto: Option<SvmUptoConfig>,
}

/// Parse one network table for a hostable family.
pub(crate) fn parse_network(
    chain_id: &ChainId,
    family: HostableFamily,
    value: toml::Value,
) -> Result<Network, Error> {
    match family {
        HostableFamily::Evm => parse_evm(chain_id, value).map(Network::Evm),
        HostableFamily::Svm => parse_svm(chain_id, value).map(Network::Svm),
    }
}

fn parse_evm(chain_id: &ChainId, value: toml::Value) -> Result<EvmNetwork, Error> {
    let raw: RawEvmNetwork = value.try_into().map_err(|err: toml::de::Error| {
        Error::config(format!("invalid [network.\"{chain_id}\"]: {err}"))
    })?;
    require_eip155_reference(chain_id)?;
    let rpc = exclusive_rpc(
        chain_id,
        raw.rpc.map(convert_evm_rpc).transpose()?,
        raw.rpc_env,
    )?;
    let schemes = require_schemes(chain_id, HostableFamily::Evm, raw.schemes)?;
    if raw.signers.is_empty() {
        return Err(Error::config(format!(
            "[network.\"{chain_id}\"] `signers` must not be empty"
        )));
    }
    Ok(EvmNetwork {
        chain_id: chain_id.clone(),
        rpc,
        signers: raw.signers,
        schemes,
        eip1559: raw.eip1559.unwrap_or(true),
        flashblocks: raw.flashblocks.unwrap_or(false),
        receipt_timeout_secs: raw
            .receipt_timeout_secs
            .unwrap_or(DEFAULT_RECEIPT_TIMEOUT_SECS),
    })
}

fn parse_svm(chain_id: &ChainId, value: toml::Value) -> Result<SvmNetwork, Error> {
    let raw: RawSvmNetwork = value.try_into().map_err(|err: toml::de::Error| {
        Error::config(format!("invalid [network.\"{chain_id}\"]: {err}"))
    })?;
    let rpc = match (raw.rpc, raw.rpc_env) {
        (Some(url), None) => RpcConfig::Literal(vec![endpoint_from_url(&url)?]),
        (None, Some(env)) => RpcConfig::Env(env),
        (None, None) | (Some(_), Some(_)) => {
            return Err(Error::config(format!(
                "[network.\"{chain_id}\"] requires exactly one of `rpc` or `rpc_env`"
            )));
        }
    };
    let schemes = require_schemes(chain_id, HostableFamily::Svm, raw.schemes)?;
    if raw.fee_payer.is_empty() {
        return Err(Error::config(format!(
            "[network.\"{chain_id}\"] `fee_payer` must not be empty"
        )));
    }
    Ok(SvmNetwork {
        chain_id: chain_id.clone(),
        rpc,
        pubsub: raw.pubsub,
        fee_payer: raw.fee_payer,
        schemes,
        max_compute_unit_limit: raw.max_compute_unit_limit.unwrap_or(DEFAULT_SVM_CU_LIMIT),
        max_compute_unit_price: raw.max_compute_unit_price,
        exact: raw.exact,
        upto: raw.upto,
    })
}

fn convert_evm_rpc(entries: Vec<RawEvmRpc>) -> Result<Vec<RpcEndpoint>, Error> {
    entries.into_iter().map(raw_evm_rpc_to_endpoint).collect()
}

fn raw_evm_rpc_to_endpoint(entry: RawEvmRpc) -> Result<RpcEndpoint, Error> {
    match entry {
        RawEvmRpc::Url(url) => endpoint_from_url(&url),
        RawEvmRpc::Endpoint(RawEvmRpcEndpoint { http, rate_limit }) => {
            let mut endpoint = endpoint_from_url(&http)?;
            endpoint.rate_limit = rate_limit;
            Ok(endpoint)
        }
    }
}

fn exclusive_rpc(
    chain_id: &ChainId,
    rpc: Option<Vec<RpcEndpoint>>,
    rpc_env: Option<String>,
) -> Result<RpcConfig, Error> {
    match (rpc, rpc_env) {
        (Some(endpoints), None) if !endpoints.is_empty() => Ok(RpcConfig::Literal(endpoints)),
        (None, Some(env)) => Ok(RpcConfig::Env(env)),
        _ => Err(Error::config(format!(
            "[network.\"{chain_id}\"] requires exactly one of `rpc` or `rpc_env`"
        ))),
    }
}

fn require_eip155_reference(chain_id: &ChainId) -> Result<(), Error> {
    if chain_id.reference().parse::<u64>().is_err() {
        return Err(Error::config(format!(
            "invalid EIP-155 reference in '{chain_id}'; expected eip155:<u64>"
        )));
    }
    Ok(())
}

fn require_schemes(
    chain_id: &ChainId,
    family: HostableFamily,
    schemes: Vec<String>,
) -> Result<Vec<String>, Error> {
    if schemes.is_empty() {
        return Err(Error::config(format!(
            "[network.\"{chain_id}\"] `schemes` must not be empty"
        )));
    }
    let known = known_scheme_names(family);
    for name in &schemes {
        if !known.contains(&name.as_str()) {
            return Err(Error::config(format!(
                "unknown scheme '{name}' for namespace '{}'",
                chain_id.namespace()
            )));
        }
    }
    Ok(schemes)
}

const fn known_scheme_names(family: HostableFamily) -> &'static [&'static str] {
    match family {
        HostableFamily::Evm => &[
            ExactScheme::VALUE,
            UptoScheme::VALUE,
            AuthCaptureScheme::VALUE,
            BatchSettlementScheme::VALUE,
        ],
        HostableFamily::Svm => &[ExactScheme::VALUE, UptoScheme::VALUE],
    }
}

fn endpoint_from_url(raw: &str) -> Result<RpcEndpoint, Error> {
    Ok(RpcEndpoint {
        url: parse_http_url(raw)?,
        rate_limit: None,
    })
}

/// Parse an RPC URL and require `http` or `https`.
pub(crate) fn parse_http_url(raw: &str) -> Result<Url, Error> {
    let url = Url::parse(raw)
        .map_err(|err| Error::config_with(format!("invalid RPC URL '{raw}'"), err))?;
    match url.scheme() {
        "http" | "https" => Ok(url),
        other => Err(Error::config(format!(
            "RPC URL '{raw}' must be http or https (got '{other}')"
        ))),
    }
}

/// Resolve `rpc_env` through `lookup`.
pub(crate) fn resolve_rpc(
    chain_id: &ChainId,
    rpc: &RpcConfig,
    lookup: &impl Fn(&str) -> Option<String>,
) -> Result<Vec<RpcEndpoint>, Error> {
    match rpc {
        RpcConfig::Literal(endpoints) => Ok(endpoints.clone()),
        RpcConfig::Env(env) => {
            let raw = lookup(env).ok_or_else(|| {
                Error::config(format!(
                    "env var '{env}' not found for [network.\"{chain_id}\"] rpc_env"
                ))
            })?;
            Ok(vec![endpoint_from_url(raw.trim())?])
        }
    }
}
