//! Per-network tables keyed by CAIP-2 id.

#[cfg(any(feature = "concordium", feature = "experimental-tron"))]
use std::time::Duration;

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

/// XRPL keys that imply a hot wallet this family does not have.
#[cfg(feature = "xrpl")]
const XRPL_HOT_WALLET_KEYS: [&str; 5] =
    ["signers", "fee_payer", "signer", "relayers", "fee_payers"];

/// Keeta has no RPC; these keys are operator mistakes.
#[cfg(feature = "keeta")]
const KEETA_RPC_KEYS: [&str; 2] = ["rpc", "rpc_env"];

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
    /// NEAR network.
    #[cfg(feature = "near")]
    Near(NearNetwork),
    /// XRPL network.
    #[cfg(feature = "xrpl")]
    Xrpl(XrplNetwork),
    /// Hedera network.
    #[cfg(feature = "hedera")]
    Hedera(HederaNetwork),
    /// Algorand network.
    #[cfg(feature = "avm")]
    Avm(AvmNetwork),
    /// Aptos network.
    #[cfg(feature = "aptos")]
    Aptos(AptosNetwork),
    /// Keeta network.
    #[cfg(feature = "keeta")]
    Keeta(KeetaNetwork),
    /// TON / TVM network.
    #[cfg(feature = "tvm")]
    Tvm(TvmNetwork),
    /// Stellar network.
    #[cfg(feature = "stellar")]
    Stellar(StellarNetwork),
    /// Concordium network.
    #[cfg(feature = "concordium")]
    Concordium(ConcordiumNetwork),
    /// Tron network.
    #[cfg(feature = "experimental-tron")]
    Tron(TronNetwork),
}

impl Network {
    /// CAIP-2 identifier.
    #[must_use]
    pub const fn chain_id(&self) -> &ChainId {
        match self {
            Self::Evm(net) => &net.chain_id,
            Self::Svm(net) => &net.chain_id,
            #[cfg(feature = "near")]
            Self::Near(net) => &net.chain_id,
            #[cfg(feature = "xrpl")]
            Self::Xrpl(net) => &net.chain_id,
            #[cfg(feature = "hedera")]
            Self::Hedera(net) => &net.chain_id,
            #[cfg(feature = "avm")]
            Self::Avm(net) => &net.chain_id,
            #[cfg(feature = "aptos")]
            Self::Aptos(net) => &net.chain_id,
            #[cfg(feature = "keeta")]
            Self::Keeta(net) => &net.chain_id,
            #[cfg(feature = "tvm")]
            Self::Tvm(net) => &net.chain_id,
            #[cfg(feature = "stellar")]
            Self::Stellar(net) => &net.chain_id,
            #[cfg(feature = "concordium")]
            Self::Concordium(net) => &net.chain_id,
            #[cfg(feature = "experimental-tron")]
            Self::Tron(net) => &net.chain_id,
        }
    }

    /// Scheme names listed on this network.
    #[must_use]
    pub fn schemes(&self) -> &[String] {
        match self {
            Self::Evm(net) => &net.schemes,
            Self::Svm(net) => &net.schemes,
            #[cfg(feature = "near")]
            Self::Near(net) => &net.schemes,
            #[cfg(feature = "xrpl")]
            Self::Xrpl(net) => &net.schemes,
            #[cfg(feature = "hedera")]
            Self::Hedera(net) => &net.schemes,
            #[cfg(feature = "avm")]
            Self::Avm(net) => &net.schemes,
            #[cfg(feature = "aptos")]
            Self::Aptos(net) => &net.schemes,
            #[cfg(feature = "keeta")]
            Self::Keeta(net) => &net.schemes,
            #[cfg(feature = "tvm")]
            Self::Tvm(net) => &net.schemes,
            #[cfg(feature = "stellar")]
            Self::Stellar(net) => &net.schemes,
            #[cfg(feature = "concordium")]
            Self::Concordium(net) => &net.schemes,
            #[cfg(feature = "experimental-tron")]
            Self::Tron(net) => &net.schemes,
        }
    }

    /// Signer names referenced by this network.
    #[must_use]
    pub fn signer_names(&self) -> &[String] {
        match self {
            Self::Evm(net) => &net.signers,
            Self::Svm(net) => std::slice::from_ref(&net.fee_payer),
            #[cfg(feature = "near")]
            Self::Near(net) => &net.relayer_signer_names,
            #[cfg(feature = "xrpl")]
            Self::Xrpl(_) => &[],
            #[cfg(feature = "hedera")]
            Self::Hedera(net) => &net.fee_payer_signer_names,
            #[cfg(feature = "avm")]
            Self::Avm(net) => &net.signers,
            #[cfg(feature = "aptos")]
            Self::Aptos(net) => &net.fee_payers,
            #[cfg(feature = "keeta")]
            Self::Keeta(net) => std::slice::from_ref(&net.signer),
            #[cfg(feature = "tvm")]
            Self::Tvm(net) => std::slice::from_ref(&net.signer),
            #[cfg(feature = "stellar")]
            Self::Stellar(net) => &net.signer_names,
            #[cfg(feature = "concordium")]
            Self::Concordium(net) => &net.signer_names,
            #[cfg(feature = "experimental-tron")]
            Self::Tron(net) => std::slice::from_ref(&net.signer),
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

/// Named account plus `[signer.*]` reference (`relayers` / `fee_payers`).
#[cfg(any(feature = "near", feature = "hedera"))]
#[derive(Debug, Clone)]
pub struct NamedAccount {
    /// On-chain account id.
    pub account_id: String,
    /// Named `[signer.*]` id.
    pub signer: String,
}

/// Parsed NEAR `[network."<caip2>"]`.
#[cfg(feature = "near")]
#[derive(Debug, Clone)]
pub struct NearNetwork {
    /// CAIP-2 id (`near:mainnet` / `near:testnet`).
    pub chain_id: ChainId,
    /// Optional RPC; `None` uses the SDK default.
    pub rpc: Option<RpcConfig>,
    /// Relayer accounts.
    pub relayers: Vec<NamedAccount>,
    /// Flattened `relayers[].signer` for `signer_names`.
    pub relayer_signer_names: Vec<String>,
    /// Scheme names (`exact`).
    pub schemes: Vec<String>,
    /// Optional sponsored-gas cap; `None` uses the SDK default.
    pub max_sponsored_gas: Option<u64>,
}

/// Parsed XRPL `[network."<caip2>"]`.
#[cfg(feature = "xrpl")]
#[derive(Debug, Clone)]
pub struct XrplNetwork {
    /// CAIP-2 id (`xrpl:0` / `xrpl:1` / `xrpl:2`).
    pub chain_id: ChainId,
    /// Optional RPC; `None` uses the SDK default for mainnet/testnet/devnet.
    pub rpc: Option<RpcConfig>,
    /// Scheme names (`exact`).
    pub schemes: Vec<String>,
    /// Optional max fee in drops; `None` uses the SDK default.
    pub max_fee_drops: Option<u64>,
}

/// Hedera `payTo` alias policy (facilitator config, not wire extra).
#[cfg(feature = "hedera")]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HederaAliasPolicy {
    /// Reject aliases (SDK default).
    #[default]
    Reject,
    /// Allow alias account creation.
    Allow,
}

/// Parsed Hedera `[network."<caip2>"]`.
#[cfg(feature = "hedera")]
#[derive(Debug, Clone)]
pub struct HederaNetwork {
    /// CAIP-2 id (`hedera:mainnet` / `hedera:testnet`).
    pub chain_id: ChainId,
    /// Fee-payer accounts.
    pub fee_payers: Vec<NamedAccount>,
    /// Flattened `fee_payers[].signer` for `signer_names`.
    pub fee_payer_signer_names: Vec<String>,
    /// Scheme names (`exact`).
    pub schemes: Vec<String>,
    /// Alias policy for `payTo`.
    pub alias_policy: HederaAliasPolicy,
    /// Optional Mirror Node REST URL.
    pub mirror_url: Option<Url>,
    /// Optional consensus-node gRPC `host:port` (`Client::for_network`).
    pub node_url: Option<String>,
}

/// Parsed Algorand `[network."<caip2>"]`.
#[cfg(feature = "avm")]
#[derive(Debug, Clone)]
pub struct AvmNetwork {
    /// CAIP-2 id (truncated genesis hash).
    pub chain_id: ChainId,
    /// Optional algod URL; `None` uses AlgoNode.
    pub algod_url: Option<Url>,
    /// Optional env var holding an algod API token.
    pub algod_token_env: Option<String>,
    /// Named `[signer.*]` fee-payer references.
    pub signers: Vec<String>,
    /// Scheme names (`exact`).
    pub schemes: Vec<String>,
    /// Confirmation wait rounds; `None` uses SDK default 10.
    pub wait_rounds: Option<u32>,
}

/// Parsed Aptos `[network."<caip2>"]`.
#[cfg(feature = "aptos")]
#[derive(Debug, Clone)]
pub struct AptosNetwork {
    /// CAIP-2 id (`aptos:1` / `aptos:2`).
    pub chain_id: ChainId,
    /// Optional RPC; `None` uses the SDK default fullnode.
    pub rpc: Option<RpcConfig>,
    /// Named `[signer.*]` fee-payer references.
    pub fee_payers: Vec<String>,
    /// Scheme names (`exact`).
    pub schemes: Vec<String>,
    /// Whether `/supported` advertises sponsorship (SDK default true).
    pub sponsor_transactions: bool,
}

/// Parsed Keeta `[network."<caip2>"]`.
#[cfg(feature = "keeta")]
#[derive(Debug, Clone)]
pub struct KeetaNetwork {
    /// CAIP-2 id (`keeta:21378` / `keeta:1413829460`).
    pub chain_id: ChainId,
    /// Named `[signer.*]` 32-byte ed25519 seed.
    pub signer: String,
    /// Derivation indices for `KeetaFeePayer::from_ed25519_seed`.
    pub indices: Vec<u32>,
    /// Scheme names (`exact`).
    pub schemes: Vec<String>,
}

/// Parsed TVM `[network."<caip2>"]`.
#[cfg(feature = "tvm")]
#[derive(Debug, Clone)]
pub struct TvmNetwork {
    /// CAIP-2 id (`tvm:-239` / `tvm:-3`).
    pub chain_id: ChainId,
    /// Optional Toncenter/TonAPI base URL; `None` uses the SDK default.
    pub provider_base_url: Option<Url>,
    /// Optional env var holding a REST API key (independent of the URL).
    pub api_key_env: Option<String>,
    /// Named `[signer.*]` Highload V3 key.
    pub signer: String,
    /// Scheme names (`exact`).
    pub schemes: Vec<String>,
    /// Highload subwallet id; `None` uses SDK `0x10ad`.
    pub subwallet_id: Option<u32>,
    /// Highload timeout seconds; `None` uses SDK `3600`.
    pub timeout: Option<u32>,
    /// Wallet workchain; `None` uses `0`.
    pub workchain: Option<i32>,
    /// Batcher idle flush interval; `None` uses the SDK default.
    pub batch_flush_interval_seconds: Option<u64>,
    /// Queue length that triggers a flush; `None` uses the SDK default.
    pub batch_flush_size: Option<usize>,
    /// Trace confirmation timeout; `None` uses the SDK default.
    pub confirmation_timeout_seconds: Option<u64>,
}

/// Parsed Stellar `[network."<caip2>"]`.
#[cfg(feature = "stellar")]
#[derive(Debug, Clone)]
pub struct StellarNetwork {
    /// CAIP-2 id (`stellar:pubnet` / `stellar:testnet`).
    pub chain_id: ChainId,
    /// Optional RPC; `None` uses the SDK default on testnet. Pubnet requires one.
    pub rpc: Option<RpcConfig>,
    /// Named inner-transaction signers.
    pub signers: Vec<String>,
    /// Optional named fee-bump signer.
    pub fee_bump: Option<String>,
    /// Flattened `signers` plus `fee_bump` for `signer_names`.
    pub signer_names: Vec<String>,
    /// Scheme names (`exact`).
    pub schemes: Vec<String>,
    /// Optional Horizon base URL.
    pub horizon_url: Option<Url>,
    /// Settlement-fee ceiling in stroops; `None` uses the SDK default.
    pub max_transaction_fee_stroops: Option<u32>,
}

/// Named Concordium account plus `[signer.*]` reference.
#[cfg(feature = "concordium")]
#[derive(Debug, Clone)]
pub struct ConcordiumAccount {
    /// `Base58Check` account address.
    pub address: String,
    /// Named `[signer.*]` id.
    pub signer: String,
}

/// Literal gRPC endpoint or env var name.
#[cfg(feature = "concordium")]
#[derive(Debug, Clone)]
pub enum GrpcConfig {
    /// URI or `host:port` from TOML.
    Literal(String),
    /// Environment variable holding a URI or `host:port`.
    Env(String),
}

/// Parsed Concordium `[network."<caip2>"]`.
#[cfg(feature = "concordium")]
#[derive(Debug, Clone)]
pub struct ConcordiumNetwork {
    /// CAIP-2 id (genesis-hash reference).
    pub chain_id: ChainId,
    /// Optional gRPC; `None` uses `default_grpc_https()`.
    pub grpc: Option<GrpcConfig>,
    /// Sponsor accounts.
    pub signers: Vec<ConcordiumAccount>,
    /// Flattened `signers[].signer` for `signer_names`.
    pub signer_names: Vec<String>,
    /// Scheme names (`exact`).
    pub schemes: Vec<String>,
    /// Whether settle waits for `ConcordiumBFT` finalization.
    pub require_finalization: bool,
    /// Finalization wait; `None` uses the SDK default.
    pub finalization_timeout: Option<Duration>,
    /// Spec Rule 7 expiry cap; `None` uses the SDK default.
    pub max_expiry_offset_seconds: Option<u64>,
}

/// Parsed Tron `[network."<caip2>"]`.
#[cfg(feature = "experimental-tron")]
#[derive(Debug, Clone)]
pub struct TronNetwork {
    /// CAIP-2 id (`tron:0x2b6653dc` / `tron:0xcd8690dc`).
    pub chain_id: ChainId,
    /// `TronGrid` base URL; exactly one of `rpc` / `rpc_env`.
    pub rpc: RpcConfig,
    /// Named `[signer.*]` secp256k1 key.
    pub signer: String,
    /// Scheme names (`exact`).
    pub schemes: Vec<String>,
    /// Settlement `fee_limit` in SUN; `None` uses `100_000_000`.
    pub fee_limit: Option<u64>,
    /// Confirmation wait; `None` uses 30s.
    pub confirmation_timeout: Option<Duration>,
    /// Confirmation poll interval; `None` uses 1s.
    pub confirmation_poll_interval: Option<Duration>,
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

#[cfg(feature = "near")]
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawNearNetwork {
    /// Literal RPC URL.
    #[serde(default)]
    rpc: Option<String>,
    /// Env name for the RPC URL.
    #[serde(default)]
    rpc_env: Option<String>,
    /// Relayer `{ account_id, signer }`.
    relayers: Vec<RawNamedAccount>,
    /// Scheme names.
    schemes: Vec<String>,
    /// Optional sponsored-gas cap.
    #[serde(default)]
    max_sponsored_gas: Option<u64>,
}

#[cfg(feature = "xrpl")]
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawXrplNetwork {
    /// Literal RPC URL.
    #[serde(default)]
    rpc: Option<String>,
    /// Env name for the RPC URL.
    #[serde(default)]
    rpc_env: Option<String>,
    /// Scheme names.
    schemes: Vec<String>,
    /// Optional max fee in drops.
    #[serde(default)]
    max_fee_drops: Option<u64>,
}

#[cfg(feature = "hedera")]
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawHederaNetwork {
    /// Fee payers `{ account_id, signer }`.
    fee_payers: Vec<RawNamedAccount>,
    /// Scheme names.
    schemes: Vec<String>,
    /// Alias policy (`reject` / `allow`).
    #[serde(default)]
    alias_policy: Option<HederaAliasPolicy>,
    /// Optional Mirror Node REST URL.
    #[serde(default)]
    mirror_url: Option<String>,
    /// Optional consensus-node gRPC `host:port`.
    #[serde(default)]
    node_url: Option<String>,
}

#[cfg(feature = "avm")]
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawAvmNetwork {
    /// Optional algod URL.
    #[serde(default)]
    algod_url: Option<String>,
    /// Optional env name for an algod API token.
    #[serde(default)]
    algod_token_env: Option<String>,
    /// Named signers.
    signers: Vec<String>,
    /// Scheme names.
    schemes: Vec<String>,
    /// Confirmation wait rounds.
    #[serde(default)]
    wait_rounds: Option<u32>,
}

#[cfg(feature = "aptos")]
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawAptosNetwork {
    /// Literal RPC URL.
    #[serde(default)]
    rpc: Option<String>,
    /// Env name for the RPC URL.
    #[serde(default)]
    rpc_env: Option<String>,
    /// Named fee-payer signers.
    fee_payers: Vec<String>,
    /// Scheme names.
    schemes: Vec<String>,
    /// Sponsorship advertised on `/supported`.
    #[serde(default)]
    sponsor_transactions: Option<bool>,
}

#[cfg(feature = "keeta")]
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawKeetaNetwork {
    /// Named `[signer.*]` seed.
    signer: String,
    /// Derivation indices.
    indices: Vec<u32>,
    /// Scheme names.
    schemes: Vec<String>,
}

#[cfg(feature = "tvm")]
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawTvmNetwork {
    /// Toncenter/TonAPI base URL override.
    #[serde(default)]
    provider_base_url: Option<String>,
    /// Alias for `provider_base_url` (mutually exclusive).
    #[serde(default)]
    rpc: Option<String>,
    /// Env name for a REST API key (not xor with the URL).
    #[serde(default)]
    api_key_env: Option<String>,
    /// Named Highload V3 signer.
    signer: String,
    /// Scheme names.
    schemes: Vec<String>,
    /// Highload subwallet id.
    #[serde(default)]
    subwallet_id: Option<u32>,
    /// Highload timeout seconds.
    #[serde(default)]
    timeout: Option<u32>,
    /// Wallet workchain.
    #[serde(default)]
    workchain: Option<i32>,
    /// Batcher idle flush interval.
    #[serde(default)]
    batch_flush_interval_seconds: Option<u64>,
    /// Queue length that triggers a flush.
    #[serde(default)]
    batch_flush_size: Option<usize>,
    /// Trace confirmation timeout.
    #[serde(default)]
    confirmation_timeout_seconds: Option<u64>,
}

#[cfg(feature = "stellar")]
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawStellarNetwork {
    /// Literal RPC URL.
    #[serde(default)]
    rpc: Option<String>,
    /// Env name for the RPC URL.
    #[serde(default)]
    rpc_env: Option<String>,
    /// Named inner-transaction signers.
    signers: Vec<String>,
    /// Optional named fee-bump signer.
    #[serde(default)]
    fee_bump: Option<String>,
    /// Scheme names.
    schemes: Vec<String>,
    /// Optional Horizon URL.
    #[serde(default)]
    horizon_url: Option<String>,
    /// Settlement-fee ceiling in stroops.
    #[serde(default)]
    max_transaction_fee_stroops: Option<u32>,
}

#[cfg(feature = "concordium")]
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawConcordiumNetwork {
    /// Literal gRPC URI or `host:port`.
    #[serde(default)]
    grpc: Option<String>,
    /// Env name for the gRPC endpoint.
    #[serde(default)]
    grpc_env: Option<String>,
    /// Sponsor `{ address, signer }`.
    signers: Vec<RawConcordiumAccount>,
    /// Scheme names.
    schemes: Vec<String>,
    /// Wait for finalization (default true).
    #[serde(default)]
    require_finalization: Option<bool>,
    /// Finalization wait timeout.
    #[serde(default, with = "humantime_serde::option")]
    finalization_timeout: Option<Duration>,
    /// Spec Rule 7 expiry cap.
    #[serde(default)]
    max_expiry_offset_seconds: Option<u64>,
}

#[cfg(feature = "concordium")]
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawConcordiumAccount {
    /// `Base58Check` account address.
    address: String,
    /// Named `[signer.*]` id.
    signer: String,
}

#[cfg(feature = "experimental-tron")]
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawTronNetwork {
    /// Literal `TronGrid` base URL.
    #[serde(default)]
    rpc: Option<String>,
    /// Env name for the `TronGrid` base URL.
    #[serde(default)]
    rpc_env: Option<String>,
    /// Named `[signer.*]` secp256k1 key.
    signer: String,
    /// Scheme names.
    schemes: Vec<String>,
    /// Settlement `fee_limit` in SUN.
    #[serde(default)]
    fee_limit: Option<u64>,
    /// Confirmation wait.
    #[serde(default, with = "humantime_serde::option")]
    confirmation_timeout: Option<Duration>,
    /// Confirmation poll interval.
    #[serde(default, with = "humantime_serde::option")]
    confirmation_poll_interval: Option<Duration>,
}

#[cfg(any(feature = "near", feature = "hedera"))]
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawNamedAccount {
    /// On-chain account id.
    account_id: String,
    /// Named `[signer.*]` id.
    signer: String,
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
        #[cfg(feature = "near")]
        HostableFamily::Near => parse_near(chain_id, value).map(Network::Near),
        #[cfg(feature = "xrpl")]
        HostableFamily::Xrpl => parse_xrpl(chain_id, value).map(Network::Xrpl),
        #[cfg(feature = "hedera")]
        HostableFamily::Hedera => parse_hedera(chain_id, value).map(Network::Hedera),
        #[cfg(feature = "avm")]
        HostableFamily::Avm => parse_avm(chain_id, value).map(Network::Avm),
        #[cfg(feature = "aptos")]
        HostableFamily::Aptos => parse_aptos(chain_id, value).map(Network::Aptos),
        #[cfg(feature = "keeta")]
        HostableFamily::Keeta => parse_keeta(chain_id, value).map(Network::Keeta),
        #[cfg(feature = "tvm")]
        HostableFamily::Tvm => parse_tvm(chain_id, value).map(Network::Tvm),
        #[cfg(feature = "stellar")]
        HostableFamily::Stellar => parse_stellar(chain_id, value).map(Network::Stellar),
        #[cfg(feature = "concordium")]
        HostableFamily::Concordium => parse_concordium(chain_id, value).map(Network::Concordium),
        #[cfg(feature = "experimental-tron")]
        HostableFamily::Tron => parse_tron(chain_id, value).map(Network::Tron),
    }
}

fn parse_evm(chain_id: &ChainId, value: toml::Value) -> Result<EvmNetwork, Error> {
    let raw: RawEvmNetwork = value.try_into().map_err(|err: toml::de::Error| {
        Error::config(format!("invalid [network.\"{chain_id}\"]: {err}"))
    })?;
    require_eip155_reference(chain_id)?;
    let rpc = require_rpc(
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
    let rpc = require_rpc(
        chain_id,
        raw.rpc.as_deref().map(single_rpc).transpose()?,
        raw.rpc_env,
    )?;
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

#[cfg(feature = "near")]
fn parse_near(chain_id: &ChainId, value: toml::Value) -> Result<NearNetwork, Error> {
    let raw: RawNearNetwork = value.try_into().map_err(|err: toml::de::Error| {
        Error::config(format!("invalid [network.\"{chain_id}\"]: {err}"))
    })?;
    r402_near::chain::NearChainReference::try_from(chain_id.clone())
        .map_err(|err| Error::config_with(format!("invalid NEAR chain id '{chain_id}'"), err))?;
    let rpc = optional_rpc(chain_id, raw.rpc, raw.rpc_env)?;
    let schemes = require_schemes(chain_id, HostableFamily::Near, raw.schemes)?;
    let (relayers, relayer_signer_names) =
        require_named_accounts(chain_id, "relayers", raw.relayers)?;
    Ok(NearNetwork {
        chain_id: chain_id.clone(),
        rpc,
        relayers,
        relayer_signer_names,
        schemes,
        max_sponsored_gas: raw.max_sponsored_gas,
    })
}

#[cfg(feature = "xrpl")]
fn parse_xrpl(chain_id: &ChainId, value: toml::Value) -> Result<XrplNetwork, Error> {
    reject_xrpl_hot_wallet(chain_id, &value)?;
    let raw: RawXrplNetwork = value.try_into().map_err(|err: toml::de::Error| {
        Error::config(format!("invalid [network.\"{chain_id}\"]: {err}"))
    })?;
    let chain = r402_xrpl::chain::XrplChainReference::try_from(chain_id.clone())
        .map_err(|err| Error::config_with(format!("invalid XRPL chain id '{chain_id}'"), err))?;
    let rpc = optional_rpc(chain_id, raw.rpc, raw.rpc_env)?;
    if rpc.is_none() && chain.default_rpc_url().is_none() {
        return Err(Error::config(format!(
            "[network.\"{chain_id}\"] requires `rpc` or `rpc_env` (no SDK default for this network)"
        )));
    }
    let schemes = require_schemes(chain_id, HostableFamily::Xrpl, raw.schemes)?;
    Ok(XrplNetwork {
        chain_id: chain_id.clone(),
        rpc,
        schemes,
        max_fee_drops: raw.max_fee_drops,
    })
}

#[cfg(feature = "hedera")]
fn parse_hedera(chain_id: &ChainId, value: toml::Value) -> Result<HederaNetwork, Error> {
    let raw: RawHederaNetwork = value.try_into().map_err(|err: toml::de::Error| {
        Error::config(format!("invalid [network.\"{chain_id}\"]: {err}"))
    })?;
    r402_hedera::chain::HederaChainReference::try_from(chain_id.clone())
        .map_err(|err| Error::config_with(format!("invalid Hedera chain id '{chain_id}'"), err))?;
    let schemes = require_schemes(chain_id, HostableFamily::Hedera, raw.schemes)?;
    let (fee_payers, fee_payer_signer_names) =
        require_named_accounts(chain_id, "fee_payers", raw.fee_payers)?;
    Ok(HederaNetwork {
        chain_id: chain_id.clone(),
        fee_payers,
        fee_payer_signer_names,
        schemes,
        alias_policy: raw.alias_policy.unwrap_or_default(),
        mirror_url: raw.mirror_url.map(|url| parse_http_url(&url)).transpose()?,
        node_url: raw
            .node_url
            .map(|url| parse_hedera_node_address(chain_id, &url))
            .transpose()?,
    })
}

#[cfg(feature = "avm")]
fn parse_avm(chain_id: &ChainId, value: toml::Value) -> Result<AvmNetwork, Error> {
    let raw: RawAvmNetwork = value.try_into().map_err(|err: toml::de::Error| {
        Error::config(format!("invalid [network.\"{chain_id}\"]: {err}"))
    })?;
    r402_avm::chain::AlgorandChainReference::try_from(chain_id.clone()).map_err(|err| {
        Error::config_with(format!("invalid Algorand chain id '{chain_id}'"), err)
    })?;
    let schemes = require_schemes(chain_id, HostableFamily::Avm, raw.schemes)?;
    if raw.signers.is_empty() {
        return Err(Error::config(format!(
            "[network.\"{chain_id}\"] `signers` must not be empty"
        )));
    }
    Ok(AvmNetwork {
        chain_id: chain_id.clone(),
        algod_url: raw.algod_url.map(|url| parse_http_url(&url)).transpose()?,
        algod_token_env: raw.algod_token_env,
        signers: raw.signers,
        schemes,
        wait_rounds: raw.wait_rounds,
    })
}

#[cfg(feature = "aptos")]
fn parse_aptos(chain_id: &ChainId, value: toml::Value) -> Result<AptosNetwork, Error> {
    let raw: RawAptosNetwork = value.try_into().map_err(|err: toml::de::Error| {
        Error::config(format!("invalid [network.\"{chain_id}\"]: {err}"))
    })?;
    r402_aptos::chain::AptosChainReference::try_from(chain_id.clone())
        .map_err(|err| Error::config_with(format!("invalid Aptos chain id '{chain_id}'"), err))?;
    let rpc = optional_rpc(chain_id, raw.rpc, raw.rpc_env)?;
    let schemes = require_schemes(chain_id, HostableFamily::Aptos, raw.schemes)?;
    if raw.fee_payers.is_empty() {
        return Err(Error::config(format!(
            "[network.\"{chain_id}\"] `fee_payers` must not be empty"
        )));
    }
    Ok(AptosNetwork {
        chain_id: chain_id.clone(),
        rpc,
        fee_payers: raw.fee_payers,
        schemes,
        sponsor_transactions: raw.sponsor_transactions.unwrap_or(true),
    })
}

#[cfg(feature = "keeta")]
fn parse_keeta(chain_id: &ChainId, value: toml::Value) -> Result<KeetaNetwork, Error> {
    reject_keeta_rpc(chain_id, &value)?;
    let raw: RawKeetaNetwork = value.try_into().map_err(|err: toml::de::Error| {
        Error::config(format!("invalid [network.\"{chain_id}\"]: {err}"))
    })?;
    r402_keeta::chain::KeetaChainReference::try_from(chain_id.clone())
        .map_err(|err| Error::config_with(format!("invalid Keeta chain id '{chain_id}'"), err))?;
    let schemes = require_schemes(chain_id, HostableFamily::Keeta, raw.schemes)?;
    if raw.signer.is_empty() {
        return Err(Error::config(format!(
            "[network.\"{chain_id}\"] `signer` must not be empty"
        )));
    }
    if raw.indices.is_empty() {
        return Err(Error::config(format!(
            "[network.\"{chain_id}\"] `indices` must not be empty"
        )));
    }
    Ok(KeetaNetwork {
        chain_id: chain_id.clone(),
        signer: raw.signer,
        indices: raw.indices,
        schemes,
    })
}

#[cfg(feature = "tvm")]
fn parse_tvm(chain_id: &ChainId, value: toml::Value) -> Result<TvmNetwork, Error> {
    let raw: RawTvmNetwork = value.try_into().map_err(|err: toml::de::Error| {
        Error::config(format!("invalid [network.\"{chain_id}\"]: {err}"))
    })?;
    r402_tvm::chain::TvmChainReference::try_from(chain_id.clone())
        .map_err(|err| Error::config_with(format!("invalid TVM chain id '{chain_id}'"), err))?;
    let provider_base_url = optional_tvm_url(chain_id, raw.provider_base_url, raw.rpc)?;
    let schemes = require_schemes(chain_id, HostableFamily::Tvm, raw.schemes)?;
    if raw.signer.is_empty() {
        return Err(Error::config(format!(
            "[network.\"{chain_id}\"] `signer` must not be empty"
        )));
    }
    Ok(TvmNetwork {
        chain_id: chain_id.clone(),
        provider_base_url,
        api_key_env: raw.api_key_env,
        signer: raw.signer,
        schemes,
        subwallet_id: raw.subwallet_id,
        timeout: raw.timeout,
        workchain: raw.workchain,
        batch_flush_interval_seconds: raw.batch_flush_interval_seconds,
        batch_flush_size: raw.batch_flush_size,
        confirmation_timeout_seconds: raw.confirmation_timeout_seconds,
    })
}

#[cfg(feature = "stellar")]
fn parse_stellar(chain_id: &ChainId, value: toml::Value) -> Result<StellarNetwork, Error> {
    let raw: RawStellarNetwork = value.try_into().map_err(|err: toml::de::Error| {
        Error::config(format!("invalid [network.\"{chain_id}\"]: {err}"))
    })?;
    let chain = r402_stellar::chain::StellarChainReference::try_from(chain_id.clone())
        .map_err(|err| Error::config_with(format!("invalid Stellar chain id '{chain_id}'"), err))?;
    let rpc = optional_rpc(chain_id, raw.rpc, raw.rpc_env)?;
    if rpc.is_none() && chain.default_rpc_url().is_none() {
        return Err(Error::config(format!(
            "[network.\"{chain_id}\"] requires `rpc` or `rpc_env` (stellar pubnet has no SDK default)"
        )));
    }
    let schemes = require_schemes(chain_id, HostableFamily::Stellar, raw.schemes)?;
    if raw.signers.is_empty() {
        return Err(Error::config(format!(
            "[network.\"{chain_id}\"] `signers` must not be empty"
        )));
    }
    let signer_names = stellar_signer_names(&raw.signers, raw.fee_bump.as_deref());
    Ok(StellarNetwork {
        chain_id: chain_id.clone(),
        rpc,
        signers: raw.signers,
        fee_bump: raw.fee_bump,
        signer_names,
        schemes,
        horizon_url: raw
            .horizon_url
            .map(|url| parse_http_url(&url))
            .transpose()?,
        max_transaction_fee_stroops: raw.max_transaction_fee_stroops,
    })
}

#[cfg(feature = "concordium")]
fn parse_concordium(chain_id: &ChainId, value: toml::Value) -> Result<ConcordiumNetwork, Error> {
    let raw: RawConcordiumNetwork = value.try_into().map_err(|err: toml::de::Error| {
        Error::config(format!("invalid [network.\"{chain_id}\"]: {err}"))
    })?;
    r402_concordium::chain::ConcordiumChainReference::try_from(chain_id.clone()).map_err(
        |err| Error::config_with(format!("invalid Concordium chain id '{chain_id}'"), err),
    )?;
    let grpc = optional_grpc(chain_id, raw.grpc, raw.grpc_env)?;
    let schemes = require_schemes(chain_id, HostableFamily::Concordium, raw.schemes)?;
    let (signers, signer_names) = require_concordium_accounts(chain_id, raw.signers)?;
    Ok(ConcordiumNetwork {
        chain_id: chain_id.clone(),
        grpc,
        signers,
        signer_names,
        schemes,
        require_finalization: raw.require_finalization.unwrap_or(true),
        finalization_timeout: raw.finalization_timeout,
        max_expiry_offset_seconds: raw.max_expiry_offset_seconds,
    })
}

#[cfg(feature = "experimental-tron")]
fn parse_tron(chain_id: &ChainId, value: toml::Value) -> Result<TronNetwork, Error> {
    let raw: RawTronNetwork = value.try_into().map_err(|err: toml::de::Error| {
        Error::config(format!("invalid [network.\"{chain_id}\"]: {err}"))
    })?;
    r402_tron::chain::TronChainReference::try_from(chain_id.clone())
        .map_err(|err| Error::config_with(format!("invalid Tron chain id '{chain_id}'"), err))?;
    let rpc = require_rpc(
        chain_id,
        raw.rpc.as_deref().map(single_rpc).transpose()?,
        raw.rpc_env,
    )?;
    let schemes = require_schemes(chain_id, HostableFamily::Tron, raw.schemes)?;
    if raw.signer.is_empty() {
        return Err(Error::config(format!(
            "[network.\"{chain_id}\"] `signer` must not be empty"
        )));
    }
    Ok(TronNetwork {
        chain_id: chain_id.clone(),
        rpc,
        signer: raw.signer,
        schemes,
        fee_limit: raw.fee_limit,
        confirmation_timeout: raw.confirmation_timeout,
        confirmation_poll_interval: raw.confirmation_poll_interval,
    })
}

#[cfg(feature = "xrpl")]
fn reject_xrpl_hot_wallet(chain_id: &ChainId, value: &toml::Value) -> Result<(), Error> {
    let Some(table) = value.as_table() else {
        return Ok(());
    };
    for key in XRPL_HOT_WALLET_KEYS {
        if table.contains_key(key) {
            return Err(Error::config(format!(
                "[network.\"{chain_id}\"] XRPL has no hot wallet; `{key}` is not valid"
            )));
        }
    }
    Ok(())
}

#[cfg(feature = "keeta")]
fn reject_keeta_rpc(chain_id: &ChainId, value: &toml::Value) -> Result<(), Error> {
    let Some(table) = value.as_table() else {
        return Ok(());
    };
    for key in KEETA_RPC_KEYS {
        if table.contains_key(key) {
            return Err(Error::config(format!(
                "[network.\"{chain_id}\"] Keeta has no RPC; `{key}` is not valid"
            )));
        }
    }
    Ok(())
}

#[cfg(feature = "stellar")]
fn stellar_signer_names(signers: &[String], fee_bump: Option<&str>) -> Vec<String> {
    let mut names = signers.to_vec();
    if let Some(bump) = fee_bump
        && !names.iter().any(|name| name == bump)
    {
        names.push(bump.to_owned());
    }
    names
}

#[cfg(feature = "concordium")]
fn require_concordium_accounts(
    chain_id: &ChainId,
    raw: Vec<RawConcordiumAccount>,
) -> Result<(Vec<ConcordiumAccount>, Vec<String>), Error> {
    if raw.is_empty() {
        return Err(Error::config(format!(
            "[network.\"{chain_id}\"] `signers` must not be empty"
        )));
    }
    let mut accounts = Vec::with_capacity(raw.len());
    let mut names = Vec::with_capacity(raw.len());
    for item in raw {
        if item.address.is_empty() {
            return Err(Error::config(format!(
                "[network.\"{chain_id}\"] `signers` entry `address` must not be empty"
            )));
        }
        if item.signer.is_empty() {
            return Err(Error::config(format!(
                "[network.\"{chain_id}\"] `signers` entry `signer` must not be empty"
            )));
        }
        names.push(item.signer.clone());
        accounts.push(ConcordiumAccount {
            address: item.address,
            signer: item.signer,
        });
    }
    Ok((accounts, names))
}

#[cfg(any(feature = "near", feature = "hedera"))]
fn require_named_accounts(
    chain_id: &ChainId,
    field: &str,
    raw: Vec<RawNamedAccount>,
) -> Result<(Vec<NamedAccount>, Vec<String>), Error> {
    if raw.is_empty() {
        return Err(Error::config(format!(
            "[network.\"{chain_id}\"] `{field}` must not be empty"
        )));
    }
    let mut accounts = Vec::with_capacity(raw.len());
    let mut names = Vec::with_capacity(raw.len());
    for item in raw {
        if item.account_id.is_empty() {
            return Err(Error::config(format!(
                "[network.\"{chain_id}\"] `{field}` entry `account_id` must not be empty"
            )));
        }
        if item.signer.is_empty() {
            return Err(Error::config(format!(
                "[network.\"{chain_id}\"] `{field}` entry `signer` must not be empty"
            )));
        }
        names.push(item.signer.clone());
        accounts.push(NamedAccount {
            account_id: item.account_id,
            signer: item.signer,
        });
    }
    Ok((accounts, names))
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

fn single_rpc(url: &str) -> Result<Vec<RpcEndpoint>, Error> {
    Ok(vec![endpoint_from_url(url)?])
}

/// EVM / SVM / Tron: exactly one of `rpc` / `rpc_env`.
fn require_rpc(
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

/// NEAR / XRPL / Aptos / Stellar: at most one of `rpc` / `rpc_env`. Omit = SDK default.
#[cfg(any(
    feature = "near",
    feature = "xrpl",
    feature = "aptos",
    feature = "stellar"
))]
fn optional_rpc(
    chain_id: &ChainId,
    rpc: Option<String>,
    rpc_env: Option<String>,
) -> Result<Option<RpcConfig>, Error> {
    match (rpc, rpc_env) {
        (Some(_), Some(_)) => Err(Error::config(format!(
            "[network.\"{chain_id}\"] accepts at most one of `rpc` or `rpc_env`"
        ))),
        (None, None) => Ok(None),
        (Some(url), None) => Ok(Some(RpcConfig::Literal(single_rpc(&url)?))),
        (None, Some(env)) => Ok(Some(RpcConfig::Env(env))),
    }
}

/// TVM: at most one of `provider_base_url` / `rpc`. Omit = Toncenter default.
#[cfg(feature = "tvm")]
fn optional_tvm_url(
    chain_id: &ChainId,
    provider_base_url: Option<String>,
    rpc: Option<String>,
) -> Result<Option<Url>, Error> {
    match (provider_base_url, rpc) {
        (Some(_), Some(_)) => Err(Error::config(format!(
            "[network.\"{chain_id}\"] accepts at most one of `provider_base_url` or `rpc`"
        ))),
        (None, None) => Ok(None),
        (Some(url), None) | (None, Some(url)) => Ok(Some(parse_http_url(&url)?)),
    }
}

/// Concordium: at most one of `grpc` / `grpc_env`. Omit = `default_grpc_https()`.
#[cfg(feature = "concordium")]
fn optional_grpc(
    chain_id: &ChainId,
    grpc: Option<String>,
    grpc_env: Option<String>,
) -> Result<Option<GrpcConfig>, Error> {
    match (grpc, grpc_env) {
        (Some(_), Some(_)) => Err(Error::config(format!(
            "[network.\"{chain_id}\"] accepts at most one of `grpc` or `grpc_env`"
        ))),
        (None, None) => Ok(None),
        (Some(endpoint), None) => Ok(Some(GrpcConfig::Literal(require_grpc_endpoint(
            chain_id, &endpoint,
        )?))),
        (None, Some(env)) => Ok(Some(GrpcConfig::Env(env))),
    }
}

#[cfg(feature = "concordium")]
fn require_grpc_endpoint(chain_id: &ChainId, raw: &str) -> Result<String, Error> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(Error::config(format!(
            "[network.\"{chain_id}\"] `grpc` must not be empty"
        )));
    }
    Ok(trimmed.to_owned())
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
        #[cfg(feature = "near")]
        HostableFamily::Near => &[ExactScheme::VALUE],
        #[cfg(feature = "xrpl")]
        HostableFamily::Xrpl => &[ExactScheme::VALUE],
        #[cfg(feature = "hedera")]
        HostableFamily::Hedera => &[ExactScheme::VALUE],
        #[cfg(feature = "avm")]
        HostableFamily::Avm => &[ExactScheme::VALUE],
        #[cfg(feature = "aptos")]
        HostableFamily::Aptos => &[ExactScheme::VALUE],
        #[cfg(feature = "keeta")]
        HostableFamily::Keeta => &[ExactScheme::VALUE],
        #[cfg(feature = "tvm")]
        HostableFamily::Tvm => &[ExactScheme::VALUE],
        #[cfg(feature = "stellar")]
        HostableFamily::Stellar => &[ExactScheme::VALUE],
        #[cfg(feature = "concordium")]
        HostableFamily::Concordium => &[ExactScheme::VALUE],
        #[cfg(feature = "experimental-tron")]
        HostableFamily::Tron => &[ExactScheme::VALUE],
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

/// Hiero `Client::for_network` keys are gRPC `host:port`, then `tcp://{host:port}`.
#[cfg(feature = "hedera")]
fn parse_hedera_node_address(chain_id: &ChainId, raw: &str) -> Result<String, Error> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(Error::config(format!(
            "[network.\"{chain_id}\"] `node_url` must be a non-empty host:port"
        )));
    }
    if trimmed.contains("://") || trimmed.chars().any(char::is_whitespace) {
        return Err(hedera_node_address_error(chain_id, trimmed));
    }
    if split_host_port(trimmed).is_none() {
        return Err(hedera_node_address_error(chain_id, trimmed));
    }
    Ok(trimmed.to_owned())
}

#[cfg(feature = "hedera")]
fn hedera_node_address_error(chain_id: &ChainId, raw: &str) -> Error {
    Error::config(format!(
        "[network.\"{chain_id}\"] `node_url` must be host:port (got '{raw}')"
    ))
}

#[cfg(feature = "hedera")]
fn split_host_port(raw: &str) -> Option<(&str, u16)> {
    if let Some(rest) = raw.strip_prefix('[') {
        let (host, port) = rest.split_once("]:")?;
        if host.is_empty() {
            return None;
        }
        let port = port.parse().ok()?;
        return Some((host, port));
    }
    let (host, port) = raw.rsplit_once(':')?;
    if host.is_empty() || host.contains(':') {
        return None;
    }
    let port = port.parse().ok()?;
    Some((host, port))
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

/// Look up an optional env-backed RPC. `None` stays the SDK default.
#[cfg(any(
    feature = "near",
    feature = "xrpl",
    feature = "aptos",
    feature = "stellar"
))]
pub(crate) fn resolve_optional_rpc(
    chain_id: &ChainId,
    rpc: Option<&RpcConfig>,
    lookup: &impl Fn(&str) -> Option<String>,
) -> Result<Option<String>, Error> {
    let Some(rpc) = rpc else {
        return Ok(None);
    };
    let endpoints = resolve_rpc(chain_id, rpc, lookup)?;
    let url = endpoints
        .first()
        .ok_or_else(|| Error::config(format!("[network.\"{chain_id}\"] has no RPC URL")))?;
    Ok(Some(url.url.as_str().to_owned()))
}

/// Look up `api_key_env` when set.
#[cfg(feature = "tvm")]
pub(crate) fn resolve_api_key(
    chain_id: &ChainId,
    api_key_env: Option<&str>,
    lookup: &impl Fn(&str) -> Option<String>,
) -> Result<Option<String>, Error> {
    let Some(env) = api_key_env else {
        return Ok(None);
    };
    let raw = lookup(env).ok_or_else(|| {
        Error::config(format!(
            "env var '{env}' not found for [network.\"{chain_id}\"] api_key_env"
        ))
    })?;
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(Error::config(format!(
            "env var '{env}' is empty for [network.\"{chain_id}\"] api_key_env"
        )));
    }
    Ok(Some(trimmed.to_owned()))
}

/// Look up optional Concordium gRPC. `None` stays `default_grpc_https()`.
#[cfg(feature = "concordium")]
pub(crate) fn resolve_optional_grpc(
    chain_id: &ChainId,
    grpc: Option<&GrpcConfig>,
    lookup: &impl Fn(&str) -> Option<String>,
) -> Result<Option<String>, Error> {
    match grpc {
        None => Ok(None),
        Some(GrpcConfig::Literal(endpoint)) => Ok(Some(endpoint.clone())),
        Some(GrpcConfig::Env(env)) => {
            let raw = lookup(env).ok_or_else(|| {
                Error::config(format!(
                    "env var '{env}' not found for [network.\"{chain_id}\"] grpc_env"
                ))
            })?;
            Ok(Some(require_grpc_endpoint(chain_id, &raw)?))
        }
    }
}

/// Look up `algod_token_env` when set.
#[cfg(feature = "avm")]
pub(crate) fn resolve_algod_token(
    chain_id: &ChainId,
    algod_token_env: Option<&str>,
    lookup: &impl Fn(&str) -> Option<String>,
) -> Result<Option<String>, Error> {
    let Some(env) = algod_token_env else {
        return Ok(None);
    };
    let raw = lookup(env).ok_or_else(|| {
        Error::config(format!(
            "env var '{env}' not found for [network.\"{chain_id}\"] algod_token_env"
        ))
    })?;
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(Error::config(format!(
            "env var '{env}' is empty for [network.\"{chain_id}\"] algod_token_env"
        )));
    }
    Ok(Some(trimmed.to_owned()))
}
