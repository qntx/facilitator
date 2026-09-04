//! `[scheme.evm]` and `[scheme.svm.*]` tables.

use serde::Deserialize;

/// Global scheme knobs. Omitted keys keep SDK defaults at construct time.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SchemeTables {
    /// EIP-155 scheme knobs shared across EVM networks.
    #[serde(default)]
    pub evm: EvmSchemeConfig,
    /// SVM scheme knobs shared across Solana networks.
    #[serde(default)]
    pub svm: SvmSchemeConfig,
}

/// `[scheme.evm]`.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EvmSchemeConfig {
    /// Clock skew tolerance in seconds.
    #[serde(default)]
    pub clock_skew_secs: Option<u64>,
    /// EIP-6492 factory allowlist (empty = fail-closed).
    #[serde(default)]
    pub eip6492_allowed_factories: Vec<String>,
    /// Whether ERC-20 approval gas sponsoring is advertised.
    #[serde(default)]
    pub erc20_approval_gas_sponsoring: bool,
    /// Optional builder-code facilitator config.
    #[serde(default)]
    pub builder_code: Option<BuilderCodeToml>,
}

/// TOML shape of `BuilderCodeFacilitatorConfig`.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BuilderCodeToml {
    /// Wallet builder code (`w`).
    #[serde(default)]
    pub builder_code: Option<String>,
    /// Facilitator service code appended to `s`.
    #[serde(default)]
    pub service_code: Option<String>,
}

/// `[scheme.svm]`.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SvmSchemeConfig {
    /// SVM exact knobs.
    #[serde(default)]
    pub exact: SvmExactConfig,
    /// SVM upto knobs.
    #[serde(default)]
    pub upto: SvmUptoConfig,
}

/// `[scheme.svm.exact]` and per-network `.exact` overrides.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SvmExactConfig {
    /// Allow extra instructions beyond the required set.
    #[serde(default)]
    pub allow_additional_instructions: Option<bool>,
    /// Instruction-count cap (SDK clamps 3..=7).
    #[serde(default)]
    pub max_instruction_count: Option<usize>,
    /// Allowed program IDs for extra instructions.
    #[serde(default)]
    pub allowed_program_ids: Option<Vec<String>>,
    /// Blocked program IDs.
    #[serde(default)]
    pub blocked_program_ids: Option<Vec<String>>,
    /// Path 2 smart-wallet verification.
    #[serde(default)]
    pub enable_smart_wallet_verification: Option<bool>,
    /// Path 2 compute-unit ceiling.
    #[serde(default)]
    pub smart_wallet_max_compute_units: Option<u32>,
    /// Path 2 priority-fee ceiling in microlamports.
    #[serde(default)]
    pub smart_wallet_max_priority_fee_micro_lamports: Option<u64>,
    /// Path 2 wallet-program allowlist.
    #[serde(default)]
    pub smart_wallet_allowed_programs: Option<Vec<String>>,
}

/// `[scheme.svm.upto]` and per-network `.upto` overrides.
#[derive(Debug, Clone, Copy, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SvmUptoConfig {
    /// Max channel lifetime in seconds.
    #[serde(default)]
    pub max_channel_lifetime_secs: Option<u64>,
    /// Max compute unit price in microlamports on open.
    #[serde(default)]
    pub max_priority_fee_micro_lamports: Option<u64>,
    /// Max compute unit limit on open.
    #[serde(default)]
    pub max_compute_units: Option<u32>,
    /// Optional required-signature ceiling.
    #[serde(default)]
    pub max_required_signatures: Option<usize>,
    /// `SetComputeUnitPrice` for facilitator settlement txs.
    #[serde(default)]
    pub compute_unit_price_micro_lamports: Option<u64>,
    /// `SetComputeUnitLimit` for facilitator settlement txs.
    #[serde(default)]
    pub settle_compute_unit_limit: Option<u32>,
}
