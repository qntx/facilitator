//! CAIP-2 namespace → Cargo feature / parse path.

#[cfg(any(feature = "evm", feature = "svm"))]
use r402_protocol::scheme::SchemeId;

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

/// Cargo feature name for a known CAIP-2 namespace.
#[must_use]
pub(crate) fn family_feature(namespace: &str) -> Option<&'static str> {
    if namespace == eip155_namespace() {
        return Some("evm");
    }
    if namespace == solana_namespace() {
        return Some("svm");
    }
    match namespace {
        "near" => Some("near"),
        "xrpl" => Some("xrpl"),
        "hedera" => Some("hedera"),
        "algorand" => Some("avm"),
        "aptos" => Some("aptos"),
        "keeta" => Some("keeta"),
        "tvm" => Some("tvm"),
        "stellar" => Some("stellar"),
        "ccd" => Some("concordium"),
        "tron" => Some("experimental-tron"),
        "casper" => Some("extra-casper"),
        _ => None,
    }
}

/// Whether `feature` is compiled into this binary.
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
        "extra-casper" => cfg!(feature = "extra-casper"),
        _ => false,
    }
}

/// Classify `namespace` for config loading.
#[must_use]
pub(crate) fn classify(namespace: &str) -> FamilyStatus {
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
    FamilyStatus::Reserved { feature }
}
