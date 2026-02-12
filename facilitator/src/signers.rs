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
        Error::signer(format!(
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

#[cfg(test)]
mod tests {
    use super::*;

    // Helper to set an env var in test context.
    //
    // SAFETY: only called from single-threaded test functions.
    #[allow(unsafe_code, clippy::disallowed_methods)]
    fn set_test_env(key: &str, value: &str) {
        unsafe { std::env::set_var(key, value) };
    }

    #[allow(unsafe_code, clippy::disallowed_methods)]
    fn remove_test_env(key: &str) {
        unsafe { std::env::remove_var(key) };
    }

    #[test]
    fn literal_value_unchanged() {
        assert_eq!(resolve_env("0x1234abcd").unwrap(), "0x1234abcd");
        assert_eq!(resolve_env("plain-text").unwrap(), "plain-text");
        assert_eq!(resolve_env("").unwrap(), "");
    }

    #[test]
    fn bare_dollar_is_literal() {
        assert_eq!(resolve_env("$").unwrap(), "$");
    }

    #[test]
    fn dollar_with_special_chars_is_literal() {
        assert_eq!(resolve_env("$not-a-var!").unwrap(), "$not-a-var!");
        assert_eq!(resolve_env("$has spaces").unwrap(), "$has spaces");
    }

    #[test]
    fn dollar_brace_syntax_resolves() {
        set_test_env("_FACILITATOR_TEST_A", "resolved_a");
        let result = resolve_env("${_FACILITATOR_TEST_A}");
        remove_test_env("_FACILITATOR_TEST_A");
        assert_eq!(result.unwrap(), "resolved_a");
    }

    #[test]
    fn dollar_syntax_resolves() {
        set_test_env("_FACILITATOR_TEST_B", "resolved_b");
        let result = resolve_env("$_FACILITATOR_TEST_B");
        remove_test_env("_FACILITATOR_TEST_B");
        assert_eq!(result.unwrap(), "resolved_b");
    }

    #[test]
    fn missing_env_var_returns_error() {
        assert!(resolve_env("${_FACILITATOR_NONEXISTENT}").is_err());
        assert!(resolve_env("$_FACILITATOR_NONEXISTENT").is_err());
    }

    #[test]
    fn resolve_string_literal() {
        let val = toml::Value::String("0xkey".into());
        let resolved = resolve_signer_value(&val).unwrap();
        assert_eq!(resolved.as_str(), Some("0xkey"));
    }

    #[test]
    fn resolve_array_of_literals() {
        let val = toml::Value::Array(vec![
            toml::Value::String("k1".into()),
            toml::Value::String("k2".into()),
        ]);
        let resolved = resolve_signer_value(&val).unwrap();
        let arr = resolved.as_array().unwrap();
        assert_eq!(arr.len(), 2);
        assert_eq!(arr[0].as_str(), Some("k1"));
        assert_eq!(arr[1].as_str(), Some("k2"));
    }

    #[test]
    fn resolve_non_string_passthrough() {
        let val = toml::Value::Integer(42);
        let resolved = resolve_signer_value(&val).unwrap();
        assert_eq!(resolved.as_integer(), Some(42));
    }

    #[test]
    fn global_evm_signers_injected() {
        let toml_str = r#"
[signers]
evm = ["0xkey1", "0xkey2"]

[chains."eip155:84532"]
rpc = [{ http = "https://example.com" }]
"#;
        let mut doc: BTreeMap<String, toml::Value> = toml::from_str(toml_str).unwrap();
        preprocess_signers(&mut doc).unwrap();

        // [signers] section must be removed after preprocessing
        assert!(!doc.contains_key("signers"));

        let chains = doc["chains"].as_table().unwrap();
        let chain = chains["eip155:84532"].as_table().unwrap();
        let signers = chain["signers"].as_array().unwrap();
        assert_eq!(signers.len(), 2);
        assert_eq!(signers[0].as_str(), Some("0xkey1"));
    }

    #[test]
    fn per_chain_signer_not_overridden() {
        let toml_str = r#"
[signers]
evm = ["0xglobal"]

[chains."eip155:84532"]
rpc = [{ http = "https://example.com" }]
signers = ["0xlocal"]
"#;
        let mut doc: BTreeMap<String, toml::Value> = toml::from_str(toml_str).unwrap();
        preprocess_signers(&mut doc).unwrap();

        let chains = doc["chains"].as_table().unwrap();
        let chain = chains["eip155:84532"].as_table().unwrap();
        let signers = chain["signers"].as_array().unwrap();
        assert_eq!(signers.len(), 1);
        assert_eq!(signers[0].as_str(), Some("0xlocal"));
    }

    #[test]
    fn global_solana_signer_injected() {
        let toml_str = r#"
[signers]
solana = "base58key"

[chains."solana:5eykt4UsFv8P8NJdTREpY1vzqKqZKvdp"]
rpc = "https://api.mainnet-beta.solana.com"
"#;
        let mut doc: BTreeMap<String, toml::Value> = toml::from_str(toml_str).unwrap();
        preprocess_signers(&mut doc).unwrap();

        let chains = doc["chains"].as_table().unwrap();
        let chain = chains["solana:5eykt4UsFv8P8NJdTREpY1vzqKqZKvdp"]
            .as_table()
            .unwrap();
        assert_eq!(chain["signer"].as_str(), Some("base58key"));
    }

    #[test]
    fn no_signers_section_is_ok() {
        let toml_str = r#"
[chains."eip155:84532"]
rpc = [{ http = "https://example.com" }]
signers = ["0xlocal"]
"#;
        let mut doc: BTreeMap<String, toml::Value> = toml::from_str(toml_str).unwrap();
        assert!(preprocess_signers(&mut doc).is_ok());
    }

    #[test]
    fn empty_chains_section_is_ok() {
        let toml_str = r#"
[signers]
evm = ["0xkey"]
"#;
        let mut doc: BTreeMap<String, toml::Value> = toml::from_str(toml_str).unwrap();
        assert!(preprocess_signers(&mut doc).is_ok());
        // [signers] should still be removed
        assert!(!doc.contains_key("signers"));
    }
}
