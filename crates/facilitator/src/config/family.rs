//! CAIP-2 namespace → Cargo feature / parse path.

#[cfg(any(
    feature = "evm",
    feature = "svm",
    feature = "near",
    feature = "xrpl",
    feature = "hedera",
    feature = "avm",
    feature = "aptos",
    feature = "keeta",
    feature = "tvm",
    feature = "stellar",
    feature = "concordium"
))]
use r402_protocol::scheme::SchemeId;

/// Casper is a remote HTTP client in r402 0.19.1; never hosted.
pub(crate) const CASPER_UNHOSTABLE: &str = "casper exact cannot be hosted: r402-casper 0.19.1 \
     CasperExactFacilitator is a remote HTTP client \
     (crates/r402-casper/src/exact/facilitator/mod.rs try_new(transport)), \
     not an on-chain facilitator";

/// Outcome of classifying a CAIP-2 namespace.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FamilyStatus {
    /// This build deserializes the family's network tables.
    Hostable(HostableFamily),
    /// Known family, feature compiled out.
    CompiledOut {
        /// Cargo feature that enables the family.
        feature: &'static str,
    },
    /// Feature is on, but constructors are not implemented in this build.
    Reserved {
        /// Cargo feature that enabled the family.
        feature: &'static str,
    },
    /// `casper:*` is a remote client, not an on-chain host.
    CasperUnhostable,
    /// Not a namespace this binary knows.
    Unknown,
}

/// Families whose tables are parsed in this build.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HostableFamily {
    /// EIP-155.
    Evm,
    /// Solana / SVM.
    Svm,
    /// NEAR.
    #[cfg(feature = "near")]
    Near,
    /// XRPL.
    #[cfg(feature = "xrpl")]
    Xrpl,
    /// Hedera.
    #[cfg(feature = "hedera")]
    Hedera,
    /// Algorand / AVM.
    #[cfg(feature = "avm")]
    Avm,
    /// Aptos.
    #[cfg(feature = "aptos")]
    Aptos,
    /// Keeta.
    #[cfg(feature = "keeta")]
    Keeta,
    /// TON / TVM.
    #[cfg(feature = "tvm")]
    Tvm,
    /// Stellar.
    #[cfg(feature = "stellar")]
    Stellar,
    /// Concordium (`ccd`).
    #[cfg(feature = "concordium")]
    Concordium,
}

/// Namespace string for EIP-155, from the SDK when the `evm` feature is on.
fn eip155_namespace() -> &'static str {
    #[cfg(feature = "evm")]
    {
        r402_evm::Eip155Exact.namespace()
    }
    #[cfg(not(feature = "evm"))]
    {
        "eip155"
    }
}

/// Namespace string for Solana, from the SDK when the `svm` feature is on.
fn solana_namespace() -> &'static str {
    #[cfg(feature = "svm")]
    {
        r402_svm::SolanaExact.namespace()
    }
    #[cfg(not(feature = "svm"))]
    {
        "solana"
    }
}

/// Namespace string for NEAR.
#[allow(
    clippy::missing_const_for_fn,
    reason = "feature-on branch calls SchemeId::namespace, which is not const"
)]
fn near_namespace() -> &'static str {
    #[cfg(feature = "near")]
    {
        r402_near::NearExact.namespace()
    }
    #[cfg(not(feature = "near"))]
    {
        "near"
    }
}

/// Namespace string for XRPL.
#[allow(
    clippy::missing_const_for_fn,
    reason = "feature-on branch calls SchemeId::namespace, which is not const"
)]
fn xrpl_namespace() -> &'static str {
    #[cfg(feature = "xrpl")]
    {
        r402_xrpl::XrplExact.namespace()
    }
    #[cfg(not(feature = "xrpl"))]
    {
        "xrpl"
    }
}

/// Namespace string for Hedera.
#[allow(
    clippy::missing_const_for_fn,
    reason = "feature-on branch calls SchemeId::namespace, which is not const"
)]
fn hedera_namespace() -> &'static str {
    #[cfg(feature = "hedera")]
    {
        r402_hedera::HederaExact.namespace()
    }
    #[cfg(not(feature = "hedera"))]
    {
        "hedera"
    }
}

/// Namespace string for Algorand (crate `r402-avm`).
#[allow(
    clippy::missing_const_for_fn,
    reason = "feature-on branch calls SchemeId::namespace, which is not const"
)]
fn algorand_namespace() -> &'static str {
    #[cfg(feature = "avm")]
    {
        r402_avm::AlgorandExact.namespace()
    }
    #[cfg(not(feature = "avm"))]
    {
        "algorand"
    }
}

/// Namespace string for Aptos.
#[allow(
    clippy::missing_const_for_fn,
    reason = "feature-on branch calls SchemeId::namespace, which is not const"
)]
fn aptos_namespace() -> &'static str {
    #[cfg(feature = "aptos")]
    {
        r402_aptos::AptosExact.namespace()
    }
    #[cfg(not(feature = "aptos"))]
    {
        "aptos"
    }
}

/// Namespace string for Keeta.
#[allow(
    clippy::missing_const_for_fn,
    reason = "feature-on branch calls SchemeId::namespace, which is not const"
)]
fn keeta_namespace() -> &'static str {
    #[cfg(feature = "keeta")]
    {
        r402_keeta::KeetaExact.namespace()
    }
    #[cfg(not(feature = "keeta"))]
    {
        "keeta"
    }
}

/// Namespace string for TON (`tvm`, not `ton`).
#[allow(
    clippy::missing_const_for_fn,
    reason = "feature-on branch calls SchemeId::namespace, which is not const"
)]
fn tvm_namespace() -> &'static str {
    #[cfg(feature = "tvm")]
    {
        r402_tvm::TvmExact.namespace()
    }
    #[cfg(not(feature = "tvm"))]
    {
        "tvm"
    }
}

/// Namespace string for Stellar.
#[allow(
    clippy::missing_const_for_fn,
    reason = "feature-on branch calls SchemeId::namespace, which is not const"
)]
fn stellar_namespace() -> &'static str {
    #[cfg(feature = "stellar")]
    {
        r402_stellar::StellarExact.namespace()
    }
    #[cfg(not(feature = "stellar"))]
    {
        "stellar"
    }
}

/// Namespace string for Concordium (`ccd`).
///
/// `ConcordiumExact` is not `Copy` and `SchemeId::namespace` is `&str`, so the
/// SDK call cannot be returned as `'static`. The hostable unit test pins this
/// against `ConcordiumExact::new().namespace()`.
const fn ccd_namespace() -> &'static str {
    "ccd"
}

/// Cargo feature name for a known CAIP-2 namespace.
#[must_use]
pub(crate) fn family_feature(namespace: &str) -> Option<&'static str> {
    if namespace == eip155_namespace() {
        return Some("evm");
    }
    if namespace == solana_namespace() {
        return Some("svm");
    }
    if namespace == near_namespace() {
        return Some("near");
    }
    if namespace == xrpl_namespace() {
        return Some("xrpl");
    }
    if namespace == hedera_namespace() {
        return Some("hedera");
    }
    if namespace == algorand_namespace() {
        return Some("avm");
    }
    if namespace == aptos_namespace() {
        return Some("aptos");
    }
    if namespace == keeta_namespace() {
        return Some("keeta");
    }
    if namespace == tvm_namespace() {
        return Some("tvm");
    }
    if namespace == stellar_namespace() {
        return Some("stellar");
    }
    if namespace == ccd_namespace() {
        return Some("concordium");
    }
    match namespace {
        "tron" => Some("experimental-tron"),
        _ => None,
    }
}

/// Whether `feature` is compiled into this binary.
#[allow(
    clippy::match_like_matches_macro,
    reason = "each arm is cfg!(feature); --all-features folds them to true"
)]
fn feature_enabled(feature: &str) -> bool {
    match feature {
        "evm" => cfg!(feature = "evm"),
        "svm" => cfg!(feature = "svm"),
        "near" => cfg!(feature = "near"),
        "xrpl" => cfg!(feature = "xrpl"),
        "hedera" => cfg!(feature = "hedera"),
        "avm" => cfg!(feature = "avm"),
        "aptos" => cfg!(feature = "aptos"),
        "keeta" => cfg!(feature = "keeta"),
        "tvm" => cfg!(feature = "tvm"),
        "stellar" => cfg!(feature = "stellar"),
        "concordium" => cfg!(feature = "concordium"),
        "experimental-tron" => cfg!(feature = "experimental-tron"),
        _ => false,
    }
}

/// Classify `namespace` for config loading.
#[must_use]
pub(crate) fn classify(namespace: &str) -> FamilyStatus {
    if namespace == "casper" {
        return FamilyStatus::CasperUnhostable;
    }
    let Some(feature) = family_feature(namespace) else {
        return FamilyStatus::Unknown;
    };
    if !feature_enabled(feature) {
        return FamilyStatus::CompiledOut { feature };
    }
    if namespace == eip155_namespace() {
        return FamilyStatus::Hostable(HostableFamily::Evm);
    }
    if namespace == solana_namespace() {
        return FamilyStatus::Hostable(HostableFamily::Svm);
    }
    #[cfg(feature = "near")]
    if namespace == near_namespace() {
        return FamilyStatus::Hostable(HostableFamily::Near);
    }
    #[cfg(feature = "xrpl")]
    if namespace == xrpl_namespace() {
        return FamilyStatus::Hostable(HostableFamily::Xrpl);
    }
    #[cfg(feature = "hedera")]
    if namespace == hedera_namespace() {
        return FamilyStatus::Hostable(HostableFamily::Hedera);
    }
    #[cfg(feature = "avm")]
    if namespace == algorand_namespace() {
        return FamilyStatus::Hostable(HostableFamily::Avm);
    }
    #[cfg(feature = "aptos")]
    if namespace == aptos_namespace() {
        return FamilyStatus::Hostable(HostableFamily::Aptos);
    }
    #[cfg(feature = "keeta")]
    if namespace == keeta_namespace() {
        return FamilyStatus::Hostable(HostableFamily::Keeta);
    }
    #[cfg(feature = "tvm")]
    if namespace == tvm_namespace() {
        return FamilyStatus::Hostable(HostableFamily::Tvm);
    }
    #[cfg(feature = "stellar")]
    if namespace == stellar_namespace() {
        return FamilyStatus::Hostable(HostableFamily::Stellar);
    }
    #[cfg(feature = "concordium")]
    if namespace == ccd_namespace() {
        return FamilyStatus::Hostable(HostableFamily::Concordium);
    }
    FamilyStatus::Reserved { feature }
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
    fn casper_is_unhostable() {
        assert_eq!(classify("casper"), FamilyStatus::CasperUnhostable);
        assert!(
            family_feature("casper").is_none(),
            "casper is not a compile feature"
        );
        assert!(
            !CASPER_UNHOSTABLE.contains("rebuild with --features"),
            "casper must not suggest a Cargo feature"
        );
        assert!(
            !CASPER_UNHOSTABLE.contains("extra-casper"),
            "extra-casper must not appear"
        );
    }

    #[test]
    fn unknown_namespace() {
        assert_eq!(classify("foo"), FamilyStatus::Unknown);
    }

    #[cfg(not(feature = "near"))]
    #[test]
    fn near_compiled_out() {
        assert_eq!(
            classify("near"),
            FamilyStatus::CompiledOut { feature: "near" }
        );
    }

    #[cfg(feature = "near")]
    #[test]
    fn near_hostable() {
        assert_eq!(
            classify("near"),
            FamilyStatus::Hostable(HostableFamily::Near)
        );
        assert_eq!(near_namespace(), r402_near::NearExact.namespace());
    }

    #[cfg(not(feature = "xrpl"))]
    #[test]
    fn xrpl_compiled_out() {
        assert_eq!(
            classify("xrpl"),
            FamilyStatus::CompiledOut { feature: "xrpl" }
        );
    }

    #[cfg(feature = "xrpl")]
    #[test]
    fn xrpl_hostable() {
        assert_eq!(
            classify("xrpl"),
            FamilyStatus::Hostable(HostableFamily::Xrpl)
        );
        assert_eq!(xrpl_namespace(), r402_xrpl::XrplExact.namespace());
    }

    #[cfg(not(feature = "hedera"))]
    #[test]
    fn hedera_compiled_out() {
        assert_eq!(
            classify("hedera"),
            FamilyStatus::CompiledOut { feature: "hedera" }
        );
    }

    #[cfg(feature = "hedera")]
    #[test]
    fn hedera_hostable() {
        assert_eq!(
            classify("hedera"),
            FamilyStatus::Hostable(HostableFamily::Hedera)
        );
        assert_eq!(hedera_namespace(), r402_hedera::HederaExact.namespace());
    }

    #[cfg(not(feature = "avm"))]
    #[test]
    fn avm_compiled_out() {
        assert_eq!(
            classify("algorand"),
            FamilyStatus::CompiledOut { feature: "avm" }
        );
    }

    #[cfg(feature = "avm")]
    #[test]
    fn avm_hostable() {
        assert_eq!(
            classify("algorand"),
            FamilyStatus::Hostable(HostableFamily::Avm)
        );
        assert_eq!(algorand_namespace(), r402_avm::AlgorandExact.namespace());
    }

    #[cfg(not(feature = "aptos"))]
    #[test]
    fn aptos_compiled_out() {
        assert_eq!(
            classify("aptos"),
            FamilyStatus::CompiledOut { feature: "aptos" }
        );
    }

    #[cfg(feature = "aptos")]
    #[test]
    fn aptos_hostable() {
        assert_eq!(
            classify("aptos"),
            FamilyStatus::Hostable(HostableFamily::Aptos)
        );
        assert_eq!(aptos_namespace(), r402_aptos::AptosExact.namespace());
    }

    #[cfg(not(feature = "keeta"))]
    #[test]
    fn keeta_compiled_out() {
        assert_eq!(
            classify("keeta"),
            FamilyStatus::CompiledOut { feature: "keeta" }
        );
    }

    #[cfg(feature = "keeta")]
    #[test]
    fn keeta_hostable() {
        assert_eq!(
            classify("keeta"),
            FamilyStatus::Hostable(HostableFamily::Keeta)
        );
        assert_eq!(keeta_namespace(), r402_keeta::KeetaExact.namespace());
    }

    #[cfg(not(feature = "tvm"))]
    #[test]
    fn tvm_compiled_out() {
        assert_eq!(
            classify("tvm"),
            FamilyStatus::CompiledOut { feature: "tvm" }
        );
        assert_eq!(
            classify("ton"),
            FamilyStatus::Unknown,
            "TVM namespace is tvm, not ton"
        );
    }

    #[cfg(feature = "tvm")]
    #[test]
    fn tvm_hostable() {
        assert_eq!(classify("tvm"), FamilyStatus::Hostable(HostableFamily::Tvm));
        assert_eq!(tvm_namespace(), r402_tvm::TvmExact.namespace());
        assert_eq!(tvm_namespace(), "tvm", "not ton");
        assert_eq!(classify("ton"), FamilyStatus::Unknown);
    }

    #[cfg(not(feature = "stellar"))]
    #[test]
    fn stellar_compiled_out() {
        assert_eq!(
            classify("stellar"),
            FamilyStatus::CompiledOut { feature: "stellar" }
        );
    }

    #[cfg(feature = "stellar")]
    #[test]
    fn stellar_hostable() {
        assert_eq!(
            classify("stellar"),
            FamilyStatus::Hostable(HostableFamily::Stellar)
        );
        assert_eq!(stellar_namespace(), r402_stellar::StellarExact.namespace());
    }

    #[cfg(not(feature = "concordium"))]
    #[test]
    fn ccd_compiled_out() {
        assert_eq!(
            classify("ccd"),
            FamilyStatus::CompiledOut {
                feature: "concordium"
            }
        );
    }

    #[cfg(feature = "concordium")]
    #[test]
    fn ccd_hostable() {
        assert_eq!(
            classify("ccd"),
            FamilyStatus::Hostable(HostableFamily::Concordium)
        );
        assert_eq!(
            ccd_namespace(),
            r402_concordium::ConcordiumExact::new().namespace()
        );
    }
}
