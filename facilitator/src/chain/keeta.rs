//! Keeta chain provider construction.

use base64::Engine as _;
use r402_core::chain::ChainId;
use r402_keeta::chain::{KeetaChainProvider, KeetaChainReference, KeetaFeePayer};
use serde::Deserialize;

use super::reject_rpc_key;
use crate::error::AppError;

/// Inner configuration for a Keeta chain (matches TOML structure).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct KeetaChainConfigInner {
    /// Hex or standard-base64 32-byte Ed25519 seed. Injected from `[signers].keeta`.
    #[serde(default)]
    pub seed: String,
    /// Derivation indices. Default `[0]`; each index yields one fee payer.
    #[serde(default = "default_keeta_indices")]
    pub indices: Vec<u32>,
}

/// Default derivation index when TOML omits `indices`.
fn default_keeta_indices() -> Vec<u32> {
    vec![0]
}

/// Full Keeta chain configuration with chain reference.
#[derive(Debug, Clone)]
pub(crate) struct KeetaChainConfig {
    /// `21378` (mainnet) or `1413829460` (testnet).
    pub chain_reference: KeetaChainReference,
    /// TOML-level configuration.
    pub inner: KeetaChainConfigInner,
}

impl KeetaChainConfig {
    /// Parse a CAIP-2 keyed chain table.
    ///
    /// # Errors
    ///
    /// Returns an error if the reference is not a known Keeta network, `rpc` is
    /// present, or the table does not match [`KeetaChainConfigInner`].
    pub(crate) fn from_toml(chain_id: &ChainId, value: toml::Value) -> Result<Self, AppError> {
        let chain_reference = KeetaChainReference::try_from(chain_id.clone())
            .map_err(|e| AppError::config_with(format!("invalid chain id '{chain_id}'"), e))?;
        reject_rpc_key(
            chain_id,
            &value,
            "Keeta uses `UserClient::from_network` (no RPC URL)",
        )?;
        let inner: KeetaChainConfigInner = value.try_into().map_err(|e: toml::de::Error| {
            AppError::config_with(format!("invalid [chains.\"{chain_id}\"]"), e)
        })?;
        Ok(Self {
            chain_reference,
            inner,
        })
    }

    /// Returns the CAIP-2 chain ID for this configuration.
    #[must_use]
    pub(crate) fn chain_id(&self) -> ChainId {
        self.chain_reference.into()
    }
}

/// 12+ whitespace-separated alphabetic words: BIP39 mnemonic, not a seed.
fn looks_like_mnemonic(secret: &str) -> bool {
    let mut count = 0usize;
    for word in secret.split_whitespace() {
        if !word.bytes().all(|b| b.is_ascii_alphabetic()) {
            return false;
        }
        count = count.saturating_add(1);
        if count >= 12 {
            return true;
        }
    }
    false
}

/// Decode a 32-byte Ed25519 seed from hex (optional `0x`) or standard-base64.
fn decode_32_byte_seed(raw: &str) -> Result<[u8; 32], AppError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(AppError::signer("keeta seed is empty"));
    }
    if looks_like_mnemonic(trimmed) {
        return Err(AppError::signer(
            "keeta seed must be hex or standard-base64 32 bytes; mnemonics are not supported",
        ));
    }
    let hex_src = trimmed
        .strip_prefix("0x")
        .or_else(|| trimmed.strip_prefix("0X"))
        .unwrap_or(trimmed);
    if hex_src.len() == 64 && hex_src.bytes().all(|b| b.is_ascii_hexdigit()) {
        return decode_hex32(hex_src);
    }
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(trimmed)
        .map_err(|e| AppError::chain_with("failed to decode keeta seed", e))?;
    <[u8; 32]>::try_from(decoded).map_err(|bytes| {
        AppError::signer(format!("keeta seed must be 32 bytes, got {}", bytes.len()))
    })
}

/// Decode 64 hex digits into 32 bytes. Caller already checked length and charset.
fn decode_hex32(hex_src: &str) -> Result<[u8; 32], AppError> {
    let mut out = [0u8; 32];
    for (slot, chunk) in out.iter_mut().zip(hex_src.as_bytes().chunks_exact(2)) {
        let s =
            std::str::from_utf8(chunk).map_err(|e| AppError::chain_with("keeta seed hex", e))?;
        *slot = u8::from_str_radix(s, 16).map_err(|e| AppError::chain_with("keeta seed hex", e))?;
    }
    Ok(out)
}

/// Build a Keeta provider from TOML.
///
/// # Errors
///
/// Returns an error if the seed is empty, looks like a mnemonic, is not 32
/// bytes, an index cannot be derived, or the network client cannot be built.
pub(crate) fn build_keeta_provider(
    config: &KeetaChainConfig,
) -> Result<KeetaChainProvider, AppError> {
    let chain_id = config.chain_id();
    if config.inner.seed.trim().is_empty() {
        return Err(AppError::chain(format!(
            "no seed configured for Keeta chain {chain_id}"
        )));
    }
    if config.inner.indices.is_empty() {
        return Err(AppError::chain(format!(
            "no indices configured for Keeta chain {chain_id}"
        )));
    }

    let seed = decode_32_byte_seed(&config.inner.seed)?;
    let mut fee_payers = Vec::with_capacity(config.inner.indices.len());
    for index in &config.inner.indices {
        let payer = KeetaFeePayer::from_ed25519_seed(seed, *index)
            .map_err(|e| AppError::chain_with("failed to derive Keeta fee payer", e))?;
        fee_payers.push(payer);
    }

    KeetaChainProvider::new(config.chain_reference, fee_payers)
        .map_err(|e| AppError::chain_with("Keeta provider init failed", e))
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;

    fn chain_id() -> ChainId {
        ChainId::from_str("keeta:1413829460").unwrap()
    }

    /// Standard-base64 of 32 zero bytes.
    const ZERO_SEED_B64: &str = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=";

    #[test]
    fn from_toml_rejects_rpc() {
        let value = toml::from_str(r#"rpc = "https://example.com""#).unwrap();
        let err = KeetaChainConfig::from_toml(&chain_id(), value).unwrap_err();
        assert!(err.to_string().contains("does not take `rpc`"), "got {err}");
    }

    #[test]
    fn from_toml_rejects_evm_shaped_rpc() {
        let value = toml::from_str(r#"rpc = [{ http = "https://example.com" }]"#).unwrap();
        let err = KeetaChainConfig::from_toml(&chain_id(), value).unwrap_err();
        assert!(err.to_string().contains("does not take `rpc`"), "got {err}");
    }

    #[test]
    fn from_toml_default_index_zero() {
        let value = toml::from_str(r#"seed = "00""#).unwrap();
        let config = KeetaChainConfig::from_toml(&chain_id(), value).unwrap();
        assert_eq!(config.inner.indices.as_slice(), &[0], "default [0]");
    }

    #[test]
    fn decode_hex_and_base64_32() {
        let hex = "00".repeat(32);
        assert_eq!(decode_32_byte_seed(&hex).unwrap(), [0u8; 32], "hex");
        assert_eq!(
            decode_32_byte_seed(&format!("0x{hex}")).unwrap(),
            [0u8; 32],
            "0x hex"
        );
        assert_eq!(
            decode_32_byte_seed(ZERO_SEED_B64).unwrap(),
            [0u8; 32],
            "b64"
        );
    }

    #[test]
    fn decode_rejects_mnemonic() {
        let err = decode_32_byte_seed(
            "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about",
        )
        .unwrap_err();
        assert!(
            err.to_string().contains("mnemonics are not supported"),
            "got {err}"
        );
    }

    #[test]
    fn build_rejects_empty_seed() {
        let value = toml::from_str(r#"seed = """#).unwrap();
        let config = KeetaChainConfig::from_toml(&chain_id(), value).unwrap();
        let err = build_keeta_provider(&config).unwrap_err();
        assert!(err.to_string().contains("no seed configured"), "got {err}");
    }

    #[test]
    fn build_rejects_empty_indices() {
        let value = toml::from_str(&format!("seed = \"{ZERO_SEED_B64}\"\nindices = []")).unwrap();
        let config = KeetaChainConfig::from_toml(&chain_id(), value).unwrap();
        let err = build_keeta_provider(&config).unwrap_err();
        assert!(
            err.to_string().contains("no indices configured"),
            "got {err}"
        );
    }

    #[test]
    fn derive_fee_payers_from_indices() {
        let seed = decode_32_byte_seed(ZERO_SEED_B64).unwrap();
        let a = KeetaFeePayer::from_ed25519_seed(seed, 0).unwrap();
        let b = KeetaFeePayer::from_ed25519_seed(seed, 1).unwrap();
        assert_ne!(a.address(), b.address(), "distinct indices");
    }
}
