//! Per-family chain builders. No unified provider enum.

#[cfg(any(
    feature = "chain-keeta",
    feature = "chain-tvm",
    feature = "chain-stellar"
))]
use r402_core::chain::ChainId;

#[cfg(any(
    feature = "chain-keeta",
    feature = "chain-tvm",
    feature = "chain-stellar"
))]
use crate::error::AppError;

#[cfg(feature = "chain-eip155")]
pub(crate) mod eip155;
#[cfg(feature = "chain-keeta")]
pub(crate) mod keeta;
#[cfg(feature = "chain-stellar")]
pub(crate) mod stellar;
#[cfg(feature = "chain-tvm")]
pub(crate) mod tvm;

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

/// Keeta constructors have no RPC string; EVM-shaped tables must not silently ignore it.
#[cfg(feature = "chain-keeta")]
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

/// TVM/Stellar `rpc` is an optional string URL, not an EVM endpoint array.
#[cfg(any(feature = "chain-tvm", feature = "chain-stellar"))]
pub(crate) fn require_string_rpc(chain_id: &ChainId, value: &toml::Value) -> Result<(), AppError> {
    match value.get("rpc") {
        None | Some(toml::Value::String(_)) => Ok(()),
        Some(_) => Err(AppError::config(format!(
            "[chains.\"{chain_id}\"] `rpc` must be a string URL"
        ))),
    }
}
