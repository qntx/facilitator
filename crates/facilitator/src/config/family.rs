//! CAIP-2 namespace → Cargo feature / parse path.

#[cfg(any(
    feature = "evm",
    feature = "svm",
    feature = "near",
    feature = "xrpl",
    feature = "hedera",
    feature = "avm"
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
    match namespace {
        "aptos" => Some("aptos"),
        "keeta" => Some("keeta"),
        "tvm" => Some("tvm"),
        "stellar" => Some("stellar"),
        "ccd" => Some("concordium"),
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

    #[test]
    fn aptos_compiled_out_or_reserved() {
        match classify("aptos") {
            FamilyStatus::CompiledOut { feature } | FamilyStatus::Reserved { feature } => {
                assert_eq!(feature, "aptos", "feature name");
            }
            other => panic!("expected compiled-out or reserved, got {other:?}"),
        }
    }
}
