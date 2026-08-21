//! Per-family chain builders. No unified provider enum.

#[cfg(any(
    feature = "chain-hedera",
    feature = "chain-algorand",
    feature = "chain-aptos",
    feature = "chain-keeta",
    feature = "chain-tvm",
    feature = "chain-stellar"
))]
use r402_core::chain::ChainId;

#[cfg(any(
    feature = "chain-hedera",
    feature = "chain-algorand",
    feature = "chain-aptos",
    feature = "chain-keeta",
    feature = "chain-tvm",
    feature = "chain-stellar"
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
#[cfg(feature = "chain-keeta")]
pub(crate) mod keeta;
#[cfg(feature = "chain-near")]
pub(crate) mod near;
#[cfg(feature = "chain-solana")]
pub(crate) mod solana;
#[cfg(feature = "chain-stellar")]
pub(crate) mod stellar;
#[cfg(feature = "chain-tvm")]
pub(crate) mod tvm;
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

/// Hedera/Algorand/Keeta constructors have no `rpc` string; EVM-shaped
/// tables must not silently ignore it.
#[cfg(any(
    feature = "chain-hedera",
    feature = "chain-algorand",
    feature = "chain-keeta"
))]
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

/// Aptos `rpc` is an optional string URL, not an EVM endpoint array.
#[cfg(feature = "chain-aptos")]
pub(crate) fn require_string_rpc(chain_id: &ChainId, value: &toml::Value) -> Result<(), AppError> {
    require_string_url(chain_id, value, "rpc")
}

/// Optional URL fields are strings, not EVM endpoint arrays.
#[cfg(any(
    feature = "chain-aptos",
    feature = "chain-tvm",
    feature = "chain-stellar"
))]
pub(crate) fn require_string_url(
    chain_id: &ChainId,
    value: &toml::Value,
    key: &str,
) -> Result<(), AppError> {
    match value.get(key) {
        None | Some(toml::Value::String(_)) => Ok(()),
        Some(_) => Err(AppError::config(format!(
            "[chains.\"{chain_id}\"] `{key}` must be a string URL"
        ))),
    }
}

/// Empty/whitespace would override r402 network defaults with a broken endpoint.
#[cfg(any(
    feature = "chain-hedera",
    feature = "chain-algorand",
    feature = "chain-aptos",
    feature = "chain-tvm",
    feature = "chain-stellar"
))]
#[must_use]
pub(crate) fn nonempty_string(value: Option<String>) -> Option<String> {
    value.and_then(|s| {
        let trimmed = s.trim();
        (!trimmed.is_empty()).then(|| trimmed.to_owned())
    })
}
