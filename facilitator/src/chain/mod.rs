//! Per-family chain builders. No unified provider enum.

#[cfg(any(
    feature = "chain-hedera",
    feature = "chain-algorand",
    feature = "chain-aptos"
))]
use r402_core::chain::ChainId;

#[cfg(any(
    feature = "chain-hedera",
    feature = "chain-algorand",
    feature = "chain-aptos"
))]
use crate::error::AppError;

#[cfg(feature = "chain-algorand")]
pub(crate) mod algorand;
#[cfg(feature = "chain-aptos")]
pub(crate) mod aptos;
#[cfg(feature = "chain-eip155")]
pub(crate) mod eip155;
#[cfg(feature = "chain-hedera")]
pub(crate) mod hedera;

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

/// Hedera/Algorand (and similar) constructors have no `rpc` string; EVM-shaped
/// tables must not silently ignore it.
#[cfg(any(feature = "chain-hedera", feature = "chain-algorand"))]
pub(crate) fn reject_rpc_key(
    chain_id: &ChainId,
    value: &toml::Value,
    hint: &str,
) -> Result<(), AppError> {
    if value.get("rpc").is_some() {
        return Err(AppError::config(format!(
            "[chains.\"{chain_id}\"] does not take `rpc`; {hint}"
        )));
    }
    Ok(())
}

/// Aptos (and similar) `rpc` is an optional string URL, not an EVM endpoint array.
#[cfg(feature = "chain-aptos")]
pub(crate) fn require_string_rpc(chain_id: &ChainId, value: &toml::Value) -> Result<(), AppError> {
    match value.get("rpc") {
        None | Some(toml::Value::String(_)) => Ok(()),
        Some(_) => Err(AppError::config(format!(
            "[chains.\"{chain_id}\"] `rpc` must be a string URL"
        ))),
    }
}

/// Empty/whitespace would override r402 network defaults with a broken endpoint.
#[cfg(any(
    feature = "chain-hedera",
    feature = "chain-algorand",
    feature = "chain-aptos"
))]
#[must_use]
pub(crate) fn nonempty_string(value: Option<String>) -> Option<String> {
    value.and_then(|s| {
        let trimmed = s.trim();
        (!trimmed.is_empty()).then(|| trimmed.to_owned())
    })
}
