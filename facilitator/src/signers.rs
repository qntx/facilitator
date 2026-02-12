//! Global signer configuration with environment variable resolution.
//!
//! This module handles the `[signers]` section of the TOML config, providing:
//!
//! - **Global signers** — a single EVM key and/or Solana key shared across all chains.
//! - **TOML pre-processing** — injects resolved signers into each chain entry
//!   before the upstream `r402` deserializer sees the config.
//!
//! # Priority
//!
//! 1. Per-chain signer (if already present in the chain table) — highest.
//! 2. Direct key in `[signers]` (`evm` / `solana` fields) — lowest.

use std::collections::BTreeMap;

use crate::error::Error;

/// Resolve an environment-variable reference (`$VAR` or `${VAR}`), returning
/// the literal string unchanged if it does not match either pattern.
fn resolve_env(value: &str) -> Result<String, Error> {
    // ${VAR} syntax — safe pattern-based extraction without byte indexing.
    if let Some(var_name) = value.strip_prefix("${").and_then(|s| s.strip_suffix('}')) {
        return lookup_env(var_name, value);
    }
    // $VAR syntax — only valid when the remainder is a well-formed identifier.
    if let Some(var_name) = value.strip_prefix('$')
        && !var_name.is_empty()
            && var_name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_')
        {
            return lookup_env(var_name, value);
        }
    // Literal value — no env-var reference detected.
    Ok(value.to_owned())
}

/// Look up an environment variable by name, returning a contextual error on
/// failure that includes both the resolved variable name and the original
/// reference string from the config file.
fn lookup_env(var_name: &str, original: &str) -> Result<String, Error> {
    std::env::var(var_name).map_err(|_| {
        Error::Signer(format!(
            "env var '{var_name}' not found (referenced as '{original}')"
        ))
    })
}

/// Resolve a signer value: if it is a string, resolve env vars; if it is an
/// array, resolve each element.
fn resolve_signer_value(val: &toml::Value) -> Result<toml::Value, Error> {
    match val {
        toml::Value::String(s) => Ok(toml::Value::String(resolve_env(s)?)),
        toml::Value::Array(arr) => {
            let resolved: Result<Vec<_>, _> = arr
                .iter()
                .map(|v| {
                    if let toml::Value::String(s) = v {
                        Ok(toml::Value::String(resolve_env(s)?))
                    } else {
                        Ok(v.clone())
                    }
                })
                .collect();
            Ok(toml::Value::Array(resolved?))
        }
        other => Ok(other.clone()),
    }
}

/// Pre-process raw TOML: extract `[signers]`, resolve env vars, and inject
/// signers into each chain entry.
///
/// Returns the TOML document (as a `BTreeMap`) ready for scheme generation and
/// final deserialization.
///
/// # Errors
///
/// Returns an error if environment variable resolution fails.
pub fn preprocess_signers(doc: &mut BTreeMap<String, toml::Value>) -> Result<(), Error> {
    let signers_table = doc.remove("signers");

    let mut evm_signers: Option<toml::Value> = None;
    let mut solana_signer: Option<toml::Value> = None;

    if let Some(toml::Value::Table(signers)) = &signers_table {
        if let Some(evm_val) = signers.get("evm") {
            evm_signers = Some(resolve_signer_value(evm_val)?);
        }
        if let Some(sol_val) = signers.get("solana") {
            solana_signer = Some(resolve_signer_value(sol_val)?);
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
