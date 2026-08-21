//! Global signer configuration with environment variable resolution.

use std::collections::BTreeMap;

use crate::error::AppError;

/// Inner implementation parameterised over the lookup function so tests can
/// supply a mock without touching process environment.
fn resolve_env_impl(
    value: &str,
    lookup: impl Fn(&str) -> Option<String>,
) -> Result<String, AppError> {
    if let Some(var_name) = value.strip_prefix("${").and_then(|s| s.strip_suffix('}')) {
        return lookup(var_name).ok_or_else(|| {
            AppError::signer(format!(
                "env var '{var_name}' not found (referenced as '{value}')"
            ))
        });
    }
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
    Ok(value.to_owned())
}

/// Recursively resolve every string in a TOML value (tables and arrays).
fn resolve_value(
    val: &toml::Value,
    lookup: &impl Fn(&str) -> Option<String>,
) -> Result<toml::Value, AppError> {
    match val {
        toml::Value::String(s) => Ok(toml::Value::String(resolve_env_impl(s, lookup)?)),
        toml::Value::Array(arr) => {
            let resolved = arr
                .iter()
                .map(|v| resolve_value(v, lookup))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(toml::Value::Array(resolved))
        }
        toml::Value::Table(table) => {
            let mut out = toml::map::Map::new();
            for (key, value) in table {
                out.insert(key.clone(), resolve_value(value, lookup)?);
            }
            Ok(toml::Value::Table(out))
        }
        other => Ok(other.clone()),
    }
}

/// Wrap a lone EVM signer string as a one-element array.
fn wrap_evm_signers(val: toml::Value) -> toml::Value {
    match val {
        toml::Value::String(s) => toml::Value::Array(vec![toml::Value::String(s)]),
        other => other,
    }
}

/// Pre-process raw TOML: extract `[signers]`, resolve env vars, inject into chains.
///
/// # Errors
///
/// Returns an error if environment variable resolution fails or `[signers].xrpl`
/// is present.
pub(crate) fn preprocess_signers(doc: &mut BTreeMap<String, toml::Value>) -> Result<(), AppError> {
    preprocess_signers_with(doc, |name| std::env::var(name).ok())
}

/// Like [`preprocess_signers`] with an injectable lookup (tests).
pub(crate) fn preprocess_signers_with(
    doc: &mut BTreeMap<String, toml::Value>,
    lookup: impl Fn(&str) -> Option<String>,
) -> Result<(), AppError> {
    let Some(signers_value) = doc.remove("signers") else {
        return Ok(());
    };
    let toml::Value::Table(signers_table) = signers_value else {
        return Err(AppError::config("[signers] must be a table"));
    };
    if signers_table.contains_key("xrpl") {
        return Err(AppError::signer(
            "[signers].xrpl is invalid; XRPL has no hot wallet",
        ));
    }

    let resolved = resolve_value(&toml::Value::Table(signers_table), &lookup)?;
    let toml::Value::Table(signers) = resolved else {
        return Err(AppError::config("[signers] must be a table"));
    };

    let evm_signers = signers.get("evm").cloned().map(wrap_evm_signers);
    let near_relayers = signers.get("near").cloned();

    if let Some(toml::Value::Table(chains)) = doc.get_mut("chains") {
        for (chain_id, chain_val) in chains.iter_mut() {
            let toml::Value::Table(chain_table) = chain_val else {
                continue;
            };
            if chain_id.starts_with("eip155:")
                && !chain_table.contains_key("signers")
                && let Some(val) = evm_signers.as_ref()
            {
                chain_table.insert("signers".to_owned(), val.clone());
            }
            if chain_id.starts_with("near:")
                && !chain_table.contains_key("relayers")
                && let Some(val) = near_relayers.as_ref()
            {
                chain_table.insert("relayers".to_owned(), val.clone());
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mock_lookup(map: &BTreeMap<String, String>) -> impl Fn(&str) -> Option<String> + '_ {
        |key| map.get(key).cloned()
    }

    #[test]
    fn literal_value_unchanged() {
        let empty = BTreeMap::new();
        assert_eq!(
            resolve_env_impl("0x1234abcd", mock_lookup(&empty)).unwrap(),
            "0x1234abcd",
            "hex literal"
        );
        assert_eq!(
            resolve_env_impl("plain-text", mock_lookup(&empty)).unwrap(),
            "plain-text",
            "plain literal"
        );
        assert_eq!(
            resolve_env_impl("", mock_lookup(&empty)).unwrap(),
            "",
            "empty literal"
        );
    }

    #[test]
    fn bare_dollar_is_literal() {
        let empty = BTreeMap::new();
        assert_eq!(
            resolve_env_impl("$", mock_lookup(&empty)).unwrap(),
            "$",
            "bare $"
        );
    }

    #[test]
    fn dollar_with_special_chars_is_literal() {
        let empty = BTreeMap::new();
        assert_eq!(
            resolve_env_impl("$not-a-var!", mock_lookup(&empty)).unwrap(),
            "$not-a-var!",
            "invalid ident"
        );
    }

    #[test]
    fn dollar_brace_syntax_resolves() {
        let env = BTreeMap::from([("MY_VAR".to_owned(), "resolved_a".to_owned())]);
        assert_eq!(
            resolve_env_impl("${MY_VAR}", mock_lookup(&env)).unwrap(),
            "resolved_a",
            "${{VAR}}"
        );
    }

    #[test]
    fn dollar_syntax_resolves() {
        let env = BTreeMap::from([("MY_VAR".to_owned(), "resolved_b".to_owned())]);
        assert_eq!(
            resolve_env_impl("$MY_VAR", mock_lookup(&env)).unwrap(),
            "resolved_b",
            "$VAR"
        );
    }

    #[test]
    fn missing_env_var_returns_error() {
        let empty = BTreeMap::new();
        assert!(
            resolve_env_impl("${NONEXISTENT}", mock_lookup(&empty)).is_err(),
            "${{missing}}"
        );
        assert!(
            resolve_env_impl("$NONEXISTENT", mock_lookup(&empty)).is_err(),
            "$missing"
        );
    }

    #[test]
    fn resolve_nested_table_strings() {
        let env = BTreeMap::from([("NEAR_KEY".to_owned(), "ed25519:secret".to_owned())]);
        let val: toml::Value = toml::from_str("near = [{ secret_key = \"$NEAR_KEY\" }]").unwrap();
        let resolved = resolve_value(&val, &mock_lookup(&env)).unwrap();
        let key = resolved
            .get("near")
            .and_then(toml::Value::as_array)
            .and_then(|a| a.first())
            .and_then(toml::Value::as_table)
            .and_then(|t| t.get("secret_key"))
            .and_then(toml::Value::as_str);
        assert_eq!(key, Some("ed25519:secret"), "nested $VAR");
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
        assert!(!doc.contains_key("signers"), "signers section removed");

        let chains = doc.get("chains").and_then(toml::Value::as_table).unwrap();
        let chain = chains
            .get("eip155:84532")
            .and_then(toml::Value::as_table)
            .unwrap();
        let signers = chain
            .get("signers")
            .and_then(toml::Value::as_array)
            .unwrap();
        assert_eq!(signers.len(), 2, "two injected keys");
        assert_eq!(
            signers.first().and_then(toml::Value::as_str),
            Some("0xkey1"),
            "first key"
        );
    }

    #[test]
    fn lone_evm_string_wrapped_to_array() {
        let toml_str = r#"
[signers]
evm = "0xlone"

[chains."eip155:84532"]
rpc = [{ http = "https://example.com" }]
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
        assert_eq!(signers.len(), 1, "wrapped");
        assert_eq!(
            signers.first().and_then(toml::Value::as_str),
            Some("0xlone"),
            "lone string"
        );
    }

    #[test]
    fn dollar_var_resolved_on_inject() {
        let toml_str = r#"
[signers]
evm = ["$EVM_KEY"]

[chains."eip155:84532"]
rpc = [{ http = "https://example.com" }]
"#;
        let mut doc: BTreeMap<String, toml::Value> = toml::from_str(toml_str).unwrap();
        let env = BTreeMap::from([("EVM_KEY".to_owned(), "0xresolved".to_owned())]);
        preprocess_signers_with(&mut doc, mock_lookup(&env)).unwrap();
        let chains = doc.get("chains").and_then(toml::Value::as_table).unwrap();
        let chain = chains
            .get("eip155:84532")
            .and_then(toml::Value::as_table)
            .unwrap();
        let signers = chain
            .get("signers")
            .and_then(toml::Value::as_array)
            .unwrap();
        assert_eq!(
            signers.first().and_then(toml::Value::as_str),
            Some("0xresolved"),
            "$VAR inject"
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
        assert_eq!(signers.len(), 1, "not overridden");
        assert_eq!(
            signers.first().and_then(toml::Value::as_str),
            Some("0xlocal"),
            "per-chain wins"
        );
    }

    #[test]
    fn xrpl_signers_are_rejected() {
        let toml_str = r#"
[signers]
xrpl = "nope"
"#;
        let mut doc: BTreeMap<String, toml::Value> = toml::from_str(toml_str).unwrap();
        let err = preprocess_signers(&mut doc).unwrap_err();
        assert!(
            err.to_string().contains("[signers].xrpl is invalid"),
            "got {err}"
        );
    }

    #[test]
    fn near_nested_var_injected() {
        let toml_str = r#"
[signers]
near = [{ account_id = "relayer.testnet", secret_key = "$NEAR_KEY" }]

[chains."near:testnet"]
"#;
        let mut doc: BTreeMap<String, toml::Value> = toml::from_str(toml_str).unwrap();
        let env = BTreeMap::from([("NEAR_KEY".to_owned(), "ed25519:secret".to_owned())]);
        preprocess_signers_with(&mut doc, mock_lookup(&env)).unwrap();
        let chains = doc.get("chains").and_then(toml::Value::as_table).unwrap();
        let chain = chains
            .get("near:testnet")
            .and_then(toml::Value::as_table)
            .unwrap();
        let relayers = chain
            .get("relayers")
            .and_then(toml::Value::as_array)
            .unwrap();
        let key = relayers
            .first()
            .and_then(toml::Value::as_table)
            .and_then(|t| t.get("secret_key"))
            .and_then(toml::Value::as_str);
        assert_eq!(key, Some("ed25519:secret"), "nested $VAR inject");
    }

    #[test]
    fn per_chain_near_relayers_not_overridden() {
        let toml_str = r#"
[signers]
near = [{ account_id = "global.testnet", secret_key = "ed25519:global" }]

[chains."near:testnet"]
relayers = [{ account_id = "local.testnet", secret_key = "ed25519:local" }]
"#;
        let mut doc: BTreeMap<String, toml::Value> = toml::from_str(toml_str).unwrap();
        preprocess_signers(&mut doc).unwrap();
        let chains = doc.get("chains").and_then(toml::Value::as_table).unwrap();
        let chain = chains
            .get("near:testnet")
            .and_then(toml::Value::as_table)
            .unwrap();
        let account = chain
            .get("relayers")
            .and_then(toml::Value::as_array)
            .and_then(|a| a.first())
            .and_then(toml::Value::as_table)
            .and_then(|t| t.get("account_id"))
            .and_then(toml::Value::as_str);
        assert_eq!(account, Some("local.testnet"), "per-chain wins");
    }

    #[test]
    fn no_signers_section_is_ok() {
        let toml_str = r#"
[chains."eip155:84532"]
rpc = [{ http = "https://example.com" }]
signers = ["0xlocal"]
"#;
        let mut doc: BTreeMap<String, toml::Value> = toml::from_str(toml_str).unwrap();
        assert!(preprocess_signers(&mut doc).is_ok(), "no [signers]");
    }
}
