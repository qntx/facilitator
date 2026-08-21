//! Per-family chain builders. No unified provider enum.

#[cfg(feature = "chain-eip155")]
pub(crate) mod eip155;

#[cfg(feature = "chain-solana")]
pub(crate) mod solana;

/// CAIP-2 namespaces this process knows, including families not compiled in.
///
/// Distinguishes a compiled-out `[chains."solana:…"]` from a typo namespace.
const KNOWN_FAMILIES: &[(&str, &str)] = &[
    ("eip155", "chain-eip155"),
    ("solana", "chain-solana"),
    ("tron", "chain-tron"),
    ("near", "chain-near"),
    ("xrpl", "chain-xrpl"),
    ("hedera", "chain-hedera"),
    ("algorand", "chain-algorand"),
    ("aptos", "chain-aptos"),
    ("keeta", "chain-keeta"),
    ("tvm", "chain-tvm"),
    ("stellar", "chain-stellar"),
];

/// Cargo feature that compiles `namespace`, if the family is known.
#[must_use]
pub(crate) fn family_feature(namespace: &str) -> Option<&'static str> {
    KNOWN_FAMILIES
        .iter()
        .find_map(|(ns, feature)| (*ns == namespace).then_some(*feature))
}
