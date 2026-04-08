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

use crate::error::AppError;

/// Resolve an environment-variable reference (`$VAR` or `${VAR}`), returning
/// the literal string unchanged if it does not match either pattern.
fn resolve_env(value: &str) -> Result<String, AppError> {
    resolve_env_impl(value, |name| std::env::var(name).ok())
}

/// Inner implementation parameterised over the lookup function so that tests
/// can supply a mock without touching real environment variables.
fn resolve_env_impl(
    value: &str,
    lookup: impl Fn(&str) -> Option<String>,
) -> Result<String, AppError> {
    // ${VAR} syntax — safe pattern-based extraction without byte indexing.
    if let Some(var_name) = value.strip_prefix("${").and_then(|s| s.strip_suffix('}')) {
        return lookup(var_name).ok_or_else(|| {
            AppError::signer(format!(
                "env var '{var_name}' not found (referenced as '{value}')"
            ))
        });
    }
    // $VAR syntax — only valid when the remainder is a well-formed identifier.
    if let Some(var_name) = value.strip_prefix('$')
        && !var_name.is_empty()
        && var_name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_')
    {
        return lookup(var_name).ok_or_else(|| {
            AppError::signer(format!(
                "env var '{var_name}' not found (referenced as '{value}')"
            ))
        });
    }
    // Literal value — no env-var reference detected.
    Ok(value.to_owned())
}

/// Resolve a signer value: if it is a string, resolve env vars; if it is an
/// array, resolve each element.
fn resolve_signer_value(val: &toml::Value) -> Result<toml::Value, AppError> {
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
pub(crate) fn preprocess_signers(doc: &mut BTreeMap<String, toml::Value>) -> Result<(), AppError> {
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
            let toml::Value::Table(chain_table) = chain_val else {
                continue;
            };
            inject_chain_signer(
                chain_id,
                chain_table,
                evm_signers.as_ref(),
                solana_signer.as_ref(),
            );
        }
    }

    Ok(())
}

/// Inject a global signer into a single chain table when no per-chain override
/// is present.
fn inject_chain_signer(
    chain_id: &str,
    table: &mut toml::map::Map<String, toml::Value>,
    evm_signers: Option<&toml::Value>,
    solana_signer: Option<&toml::Value>,
) {
    if chain_id.starts_with("eip155:") && !table.contains_key("signers") {
        if let Some(val) = evm_signers {
            table.insert("signers".to_owned(), val.clone());
        }
    } else if chain_id.starts_with("solana:")
        && !table.contains_key("signer")
        && let Some(val) = solana_signer
    {
        table.insert("signer".to_owned(), val.clone());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Mock lookup: returns the value from a fixed map.
    fn mock_lookup(map: &BTreeMap<String, String>) -> impl Fn(&str) -> Option<String> + '_ {
        |key| map.get(key).cloned()
    }

    #[test]
    fn literal_value_unchanged() {
        let empty = BTreeMap::new();
        assert_eq!(
            resolve_env_impl("0x1234abcd", mock_lookup(&empty)).unwrap(),
            "0x1234abcd"
        );
        assert_eq!(
            resolve_env_impl("plain-text", mock_lookup(&empty)).unwrap(),
            "plain-text"
        );
        assert_eq!(resolve_env_impl("", mock_lookup(&empty)).unwrap(), "");
    }

    #[test]
    fn bare_dollar_is_literal() {
        let empty = BTreeMap::new();
        assert_eq!(resolve_env_impl("$", mock_lookup(&empty)).unwrap(), "$");
    }

    #[test]
    fn dollar_with_special_chars_is_literal() {
        let empty = BTreeMap::new();
        assert_eq!(
            resolve_env_impl("$not-a-var!", mock_lookup(&empty)).unwrap(),
            "$not-a-var!"
        );
        assert_eq!(
            resolve_env_impl("$has spaces", mock_lookup(&empty)).unwrap(),
            "$has spaces"
        );
    }

    #[test]
    fn dollar_brace_syntax_resolves() {
        let env = BTreeMap::from([("MY_VAR".to_owned(), "resolved_a".to_owned())]);
        assert_eq!(
            resolve_env_impl("${MY_VAR}", mock_lookup(&env)).unwrap(),
            "resolved_a"
        );
    }

    #[test]
    fn dollar_syntax_resolves() {
        let env = BTreeMap::from([("MY_VAR".to_owned(), "resolved_b".to_owned())]);
        assert_eq!(
            resolve_env_impl("$MY_VAR", mock_lookup(&env)).unwrap(),
            "resolved_b"
        );
    }

    #[test]
    fn missing_env_var_returns_error() {
        let empty = BTreeMap::new();
        assert!(resolve_env_impl("${NONEXISTENT}", mock_lookup(&empty)).is_err());
        assert!(resolve_env_impl("$NONEXISTENT", mock_lookup(&empty)).is_err());
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
        assert_eq!(arr.first().and_then(toml::Value::as_str), Some("k1"));
        assert_eq!(arr.get(1).and_then(toml::Value::as_str), Some("k2"));
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

        let chains = doc.get("chains").and_then(toml::Value::as_table).unwrap();
        let chain = chains
            .get("eip155:84532")
            .and_then(toml::Value::as_table)
            .unwrap();
        let signers = chain
            .get("signers")
            .and_then(toml::Value::as_array)
            .unwrap();
        assert_eq!(signers.len(), 2);
        assert_eq!(
            signers.first().and_then(toml::Value::as_str),
            Some("0xkey1")
        );
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

        let chains = doc.get("chains").and_then(toml::Value::as_table).unwrap();
        let chain = chains
            .get("eip155:84532")
            .and_then(toml::Value::as_table)
            .unwrap();
        let signers = chain
            .get("signers")
            .and_then(toml::Value::as_array)
            .unwrap();
        assert_eq!(signers.len(), 1);
        assert_eq!(
            signers.first().and_then(toml::Value::as_str),
            Some("0xlocal")
        );
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

        let chains = doc.get("chains").and_then(toml::Value::as_table).unwrap();
        let chain = chains
            .get("solana:5eykt4UsFv8P8NJdTREpY1vzqKqZKvdp")
            .and_then(toml::Value::as_table)
            .unwrap();
        assert_eq!(
            chain.get("signer").and_then(toml::Value::as_str),
            Some("base58key")
        );
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
