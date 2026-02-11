//! Global signer configuration and mnemonic-based key derivation.
//!
//! This module handles the `[signers]` section of the TOML config, providing:
//!
//! - **Global signers** — a single EVM key and/or Solana key shared across all chains.
//! - **Mnemonic derivation** — BIP-44 (EVM) and SLIP-10 (Solana) key derivation
//!   from a BIP-39 mnemonic phrase via the [`kobe`] crate family.
//! - **TOML pre-processing** — injects resolved signers into each chain entry
//!   before the upstream `r402` deserializer sees the config.
//!
//! # Priority
//!
//! 1. Per-chain signer (if already present in the chain table) — highest.
//! 2. Direct key in `[signers]` (`evm` / `solana` fields).
//! 3. Mnemonic-derived key (`mnemonic` field) — lowest.

use std::collections::BTreeMap;

use crate::error::Error;

/// Default EVM BIP-44 derivation path (`MetaMask` / Trezor compatible).
#[cfg(feature = "chain-eip155")]
const DEFAULT_EVM_PATH: &str = "m/44'/60'/0'/0/0";

/// Default Solana SLIP-10 derivation path (Phantom / Backpack compatible).
#[cfg(feature = "chain-solana")]
const DEFAULT_SOLANA_PATH: &str = "m/44'/501'/0'/0'";

/// Resolve an environment-variable reference (`$VAR` or `${VAR}`), returning
/// the literal string unchanged if it does not match either pattern.
fn resolve_env(value: &str) -> Result<String, Error> {
    // ${VAR} syntax
    if value.starts_with("${") && value.ends_with('}') {
        let var_name = &value[2..value.len() - 1];
        return std::env::var(var_name).map_err(|_| {
            Error::Signer(format!(
                "env var '{var_name}' not found (referenced as '{value}')"
            ))
        });
    }
    // $VAR syntax
    if value.starts_with('$') && value.len() > 1 {
        let var_name = &value[1..];
        if var_name.chars().all(|c| c.is_alphanumeric() || c == '_') {
            return std::env::var(var_name).map_err(|_| {
                Error::Signer(format!(
                    "env var '{var_name}' not found (referenced as '{value}')"
                ))
            });
        }
    }
    // Literal value
    Ok(value.to_owned())
}

/// Derive an EVM private key (0x-prefixed hex) from a mnemonic phrase.
#[cfg(feature = "chain-eip155")]
fn derive_evm_key(
    mnemonic: &str,
    passphrase: Option<&str>,
    path: Option<&str>,
) -> Result<String, Error> {
    let wallet = kobe::Wallet::from_mnemonic(mnemonic, passphrase)
        .map_err(|e| Error::Signer(format!("mnemonic parse error: {e}")))?;
    let deriver = kobe_eth::Deriver::new(&wallet);
    let derived = deriver
        .derive_path(path.unwrap_or(DEFAULT_EVM_PATH))
        .map_err(|e| Error::Signer(format!("EVM key derivation error: {e}")))?;
    Ok(format!("0x{}", &*derived.private_key_hex))
}

/// Derive a Solana keypair (base58, 64-byte secret+public) from a mnemonic.
#[cfg(feature = "chain-solana")]
fn derive_solana_key(
    mnemonic: &str,
    passphrase: Option<&str>,
    path: Option<&str>,
) -> Result<String, Error> {
    let wallet = kobe::Wallet::from_mnemonic(mnemonic, passphrase)
        .map_err(|e| Error::Signer(format!("mnemonic parse error: {e}")))?;
    let deriver = kobe_sol::Deriver::new(&wallet);
    let derived = deriver
        .derive_path(path.unwrap_or(DEFAULT_SOLANA_PATH))
        .map_err(|e| Error::Signer(format!("Solana key derivation error: {e}")))?;

    // Upstream SolanaPrivateKey expects 64-byte base58: [secret(32) | public(32)]
    let secret_bytes = hex::decode(&*derived.private_key_hex)
        .map_err(|e| Error::Signer(format!("hex decode error: {e}")))?;
    let public_bytes = hex::decode(&derived.public_key_hex)
        .map_err(|e| Error::Signer(format!("hex decode error: {e}")))?;

    let mut keypair = Vec::with_capacity(64);
    keypair.extend_from_slice(&secret_bytes);
    keypair.extend_from_slice(&public_bytes);
    Ok(bs58::encode(&keypair).into_string())
}

/// Pre-process raw TOML: extract `[signers]`, resolve env vars, derive keys,
/// and inject signers into each chain entry.
///
/// Returns the TOML document (as a `BTreeMap`) ready for scheme generation and
/// final deserialization.
///
/// # Errors
///
/// Returns an error if environment variable resolution, mnemonic parsing,
/// or key derivation fails.
pub fn preprocess_signers(doc: &mut BTreeMap<String, toml::Value>) -> Result<(), Error> {
    let signers_table = doc.remove("signers");

    #[allow(unused_mut)]
    let mut evm_signers: Option<toml::Value> = None;
    #[allow(unused_mut)]
    let mut solana_signer: Option<toml::Value> = None;

    if let Some(toml::Value::Table(signers)) = &signers_table {
        if let Some(evm_val) = signers.get("evm") {
            evm_signers = Some(evm_val.clone());
        }
        if let Some(sol_val) = signers.get("solana") {
            solana_signer = Some(sol_val.clone());
        }

        if let Some(toml::Value::String(mnemonic_raw)) = signers.get("mnemonic") {
            let mnemonic = resolve_env(mnemonic_raw)?;
            let passphrase = signers
                .get("passphrase")
                .and_then(|v| v.as_str())
                .map(resolve_env)
                .transpose()?;
            let passphrase_ref = passphrase.as_deref();

            #[cfg(feature = "chain-eip155")]
            if evm_signers.is_none() {
                let evm_path = signers.get("evm_derivation_path").and_then(|v| v.as_str());
                let key = derive_evm_key(&mnemonic, passphrase_ref, evm_path)?;
                evm_signers = Some(toml::Value::Array(vec![toml::Value::String(key)]));
            }

            #[cfg(feature = "chain-solana")]
            if solana_signer.is_none() {
                let sol_path = signers
                    .get("solana_derivation_path")
                    .and_then(|v| v.as_str());
                let key = derive_solana_key(&mnemonic, passphrase_ref, sol_path)?;
                solana_signer = Some(toml::Value::String(key));
            }
        }
    }

    // Inject global signers into chain entries that don't have their own
    if let Some(toml::Value::Table(chains)) = doc.get_mut("chains") {
        for (chain_id, chain_val) in chains.iter_mut() {
            if let toml::Value::Table(chain_table) = chain_val {
                if chain_id.starts_with("eip155:") {
                    if !chain_table.contains_key("signers")
                        && let Some(ref signers_val) = evm_signers
                    {
                        chain_table.insert("signers".to_owned(), signers_val.clone());
                    }
                } else if chain_id.starts_with("solana:")
                    && !chain_table.contains_key("signer")
                    && let Some(ref signer_val) = solana_signer
                {
                    chain_table.insert("signer".to_owned(), signer_val.clone());
                }
            }
        }
    }

    Ok(())
}
