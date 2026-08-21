//! Per-family chain builders. No unified provider enum.

#[cfg(feature = "chain-eip155")]
pub(crate) mod eip155;

#[cfg(feature = "chain-near")]
pub(crate) mod near;

#[cfg(feature = "chain-xrpl")]
pub(crate) mod xrpl;

/// CAIP-2 namespaces this process knows, including families not compiled in.
///
/// Distinguishes a compiled-out `[chains."solana:…"]` from a typo namespace.
/// Hostable families only; blocked namespaces use `blocked_family`.
const KNOWN_FAMILIES: &[(&str, &str)] = &[
    ("eip155", "chain-eip155"),
    ("solana", "chain-solana"),
    ("near", "chain-near"),
    ("xrpl", "chain-xrpl"),
    ("hedera", "chain-hedera"),
    ("algorand", "chain-algorand"),
    ("aptos", "chain-aptos"),
    ("keeta", "chain-keeta"),
    ("tvm", "chain-tvm"),
    ("stellar", "chain-stellar"),
];

/// r402 in-process families this binary cannot host (no Cargo feature exists).
const BLOCKED_FAMILIES: &[(&str, &str)] = &[(
    "tron",
    "not hosted on r402 0.17.1: no SchemeBuilder<&TronChainProvider> \
     (TronChainProvider is not Clone; a local impl is orphan-illegal)",
)];

/// Cargo feature that compiles `namespace`, if the family is known and hostable.
#[must_use]
pub(crate) fn family_feature(namespace: &str) -> Option<&'static str> {
    KNOWN_FAMILIES
        .iter()
        .find_map(|(ns, feature)| (*ns == namespace).then_some(*feature))
}

/// Why `namespace` cannot be hosted, if it is a blocked family.
#[must_use]
pub(crate) fn blocked_family(namespace: &str) -> Option<&'static str> {
    BLOCKED_FAMILIES
        .iter()
        .find_map(|(ns, reason)| (*ns == namespace).then_some(*reason))
}
