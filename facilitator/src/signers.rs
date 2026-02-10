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

/// Default EVM BIP-44 derivation path (`MetaMask` / Trezor compatible).
#[cfg(feature = "chain-eip155")]
const DEFAULT_EVM_PATH: &str = "m/44'/60'/0'/0/0";

/// Default Solana SLIP-10 derivation path (Phantom / Backpack compatible).
#[cfg(feature = "chain-solana")]
const DEFAULT_SOLANA_PATH: &str = "m/44'/501'/0'/0'";

/// Resolve an environment-variable reference (`$VAR` or `${VAR}`), returning
/// the literal string unchanged if it does not match either pattern.
fn resolve_env(value: &str) -> Result<String, Box<dyn std::error::Error>> {
    // ${VAR} syntax
    if value.starts_with("${") && value.ends_with('}') {
        let var_name = &value[2..value.len() - 1];
        return std::env::var(var_name).map_err(|_| {
            format!("Environment variable '{var_name}' not found (referenced as '{value}')").into()
        });
    }
    // $VAR syntax
    if value.starts_with('$') && value.len() > 1 {
        let var_name = &value[1..];
        if var_name.chars().all(|c| c.is_alphanumeric() || c == '_') {
            return std::env::var(var_name).map_err(|_| {
                format!("Environment variable '{var_name}' not found (referenced as '{value}')")
                    .into()
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
) -> Result<String, Box<dyn std::error::Error>> {
    let wallet = kobe::Wallet::from_mnemonic(mnemonic, passphrase)?;
    let deriver = kobe_eth::Deriver::new(&wallet);
    let derived = if let Some(custom_path) = path {
        deriver.derive_path(custom_path)?
    } else {
        deriver.derive_path(DEFAULT_EVM_PATH)?
    };
    Ok(format!("0x{}", &*derived.private_key_hex))
}

/// Derive a Solana keypair (base58, 64-byte secret+public) from a mnemonic.
#[cfg(feature = "chain-solana")]
fn derive_solana_key(
    mnemonic: &str,
    passphrase: Option<&str>,
    path: Option<&str>,
) -> Result<String, Box<dyn std::error::Error>> {
    let wallet = kobe::Wallet::from_mnemonic(mnemonic, passphrase)?;
    let deriver = kobe_sol::Deriver::new(&wallet);
    let derived = if let Some(custom_path) = path {
        deriver.derive_path(custom_path)?
    } else {
        deriver.derive_path(DEFAULT_SOLANA_PATH)?
    };

    // Upstream SolanaPrivateKey expects 64-byte base58: [secret(32) | public(32)]
    let secret_bytes =
        hex::decode(&*derived.private_key_hex).map_err(|e| format!("hex decode error: {e}"))?;
    let public_bytes =
        hex::decode(&derived.public_key_hex).map_err(|e| format!("hex decode error: {e}"))?;

    let mut keypair = Vec::with_capacity(64);
    keypair.extend_from_slice(&secret_bytes);
    keypair.extend_from_slice(&public_bytes);
    Ok(bs58::encode(&keypair).into_string())
}

/// Pre-process raw TOML: extract `[signers]`, derive keys if needed, inject
/// into each chain entry, and auto-generate `[[schemes]]` when absent.
///
/// Returns the modified TOML string ready for upstream deserialization.
///
/// # Errors
///
/// Returns an error if environment variable resolution, mnemonic parsing,
/// or key derivation fails.
pub fn preprocess_config(raw: &str) -> Result<String, Box<dyn std::error::Error>> {
    let mut doc: BTreeMap<String, toml::Value> =
        toml::from_str(raw).map_err(|e| format!("TOML parse error: {e}"))?;

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
                .map(|s| resolve_env(s))
                .transpose()?;
            let passphrase_ref = passphrase.as_deref();

            // Derive EVM key from mnemonic if no direct evm key provided
            #[cfg(feature = "chain-eip155")]
            if evm_signers.is_none() {
                let evm_path = signers.get("evm_derivation_path").and_then(|v| v.as_str());
                let key = derive_evm_key(&mnemonic, passphrase_ref, evm_path)?;
                evm_signers = Some(toml::Value::Array(vec![toml::Value::String(key)]));
            }

            // Derive Solana key from mnemonic if no direct solana key provided
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

    #[allow(unused_mut)]
    let mut evm_chain_ids: Vec<String> = Vec::new();
    #[allow(unused_mut)]
    let mut solana_chain_ids: Vec<String> = Vec::new();

    if let Some(toml::Value::Table(chains)) = doc.get_mut("chains") {
        for (chain_id, chain_val) in chains.iter_mut() {
            if let toml::Value::Table(chain_table) = chain_val {
                if chain_id.starts_with("eip155:") {
                    evm_chain_ids.push(chain_id.clone());
                    // Inject global EVM signers if chain doesn't have its own
                    if !chain_table.contains_key("signers")
                        && let Some(ref signers_val) = evm_signers
                    {
                        chain_table.insert("signers".to_owned(), signers_val.clone());
                    }
                } else if chain_id.starts_with("solana:") {
                    solana_chain_ids.push(chain_id.clone());
                    // Inject global Solana signer if chain doesn't have its own
                    if !chain_table.contains_key("signer")
                        && let Some(ref signer_val) = solana_signer
                    {
                        chain_table.insert("signer".to_owned(), signer_val.clone());
                    }
                }
            }
        }
    }

    let needs_schemes = match doc.get("schemes") {
        None => true,
        Some(toml::Value::Array(arr)) => arr.is_empty(),
        _ => false,
    };

    if needs_schemes {
        let mut schemes = Vec::new();

        #[cfg(feature = "chain-eip155")]
        if !evm_chain_ids.is_empty() {
            let chain_values: Vec<toml::Value> = evm_chain_ids
                .iter()
                .map(|id| toml::Value::String(id.clone()))
                .collect();

            for scheme_name in &["v1-eip155-exact", "v2-eip155-exact"] {
                let mut entry = toml::map::Map::new();
                entry.insert(
                    "scheme".to_owned(),
                    toml::Value::String((*scheme_name).to_owned()),
                );
                entry.insert(
                    "chains".to_owned(),
                    toml::Value::Array(chain_values.clone()),
                );
                schemes.push(toml::Value::Table(entry));
            }
        }

        #[cfg(feature = "chain-solana")]
        if !solana_chain_ids.is_empty() {
            let chain_values: Vec<toml::Value> = solana_chain_ids
                .iter()
                .map(|id| toml::Value::String(id.clone()))
                .collect();

            for scheme_name in &["v1-solana-exact", "v2-solana-exact"] {
                let mut entry = toml::map::Map::new();
                entry.insert(
                    "scheme".to_owned(),
                    toml::Value::String((*scheme_name).to_owned()),
                );
                entry.insert(
                    "chains".to_owned(),
                    toml::Value::Array(chain_values.clone()),
                );
                schemes.push(toml::Value::Table(entry));
            }
        }

        if !schemes.is_empty() {
            doc.insert("schemes".to_owned(), toml::Value::Array(schemes));
        }
    }

    toml::to_string(&doc).map_err(|e| format!("Failed to serialize processed config: {e}").into())
}
