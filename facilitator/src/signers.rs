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

/// Wrap a lone string as a one-element array (Stellar secrets).
fn wrap_as_array(val: toml::Value) -> toml::Value {
    match val {
        toml::Value::Array(_) => val,
        other => toml::Value::Array(vec![other]),
    }
}

/// Insert `value` under `field` when the chain table does not already set it.
fn inject_field(
    chain_table: &mut toml::map::Map<String, toml::Value>,
    field: &str,
    value: Option<&toml::Value>,
) {
    if chain_table.contains_key(field) {
        return;
    }
    if let Some(val) = value {
        chain_table.insert(field.to_owned(), val.clone());
    }
}

/// Apply namespace-prefix injections into each `[chains.*]` table.
fn inject_resolved_signers(
    chains: &mut toml::map::Map<String, toml::Value>,
    injections: &[(&str, &str, Option<&toml::Value>)],
) {
    for (chain_id, chain_val) in chains.iter_mut() {
        let toml::Value::Table(chain_table) = chain_val else {
            continue;
        };
        for &(prefix, field, value) in injections {
            if chain_id.starts_with(prefix) {
                inject_field(chain_table, field, value);
            }
        }
    }
}

/// Inject Keeta `seed` / `indices` independently; per-chain keys win.
fn inject_keeta_seed_and_indices(
    chain_table: &mut toml::map::Map<String, toml::Value>,
    keeta: Option<&toml::Value>,
) {
    match keeta {
        Some(toml::Value::String(s)) => {
            let seed = toml::Value::String(s.clone());
            inject_field(chain_table, "seed", Some(&seed));
        }
        Some(toml::Value::Table(table)) => {
            inject_field(chain_table, "seed", table.get("seed"));
            inject_field(chain_table, "indices", table.get("indices"));
        }
        _ => {}
    }
}

/// `[signers].tvm` / `stellar_fee_bump` are a single secret string.
fn require_signer_string(name: &str, value: Option<&toml::Value>) -> Result<(), AppError> {
    match value {
        None | Some(toml::Value::String(_)) => Ok(()),
        Some(_) => Err(AppError::config(format!(
            "[signers].{name} must be a string"
        ))),
    }
}

/// `[signers].stellar` is one `S…` secret or an array of them.
fn require_signer_string_or_array(name: &str, value: Option<&toml::Value>) -> Result<(), AppError> {
    match value {
        None | Some(toml::Value::String(_) | toml::Value::Array(_)) => Ok(()),
        Some(_) => Err(AppError::config(format!(
            "[signers].{name} must be a string or array of S… secrets"
        ))),
    }
}

/// Pre-process raw TOML: extract `[signers]`, resolve env vars, inject into chains.
///
/// # Errors
///
/// Returns an error if environment variable resolution fails, `[signers].xrpl`
/// is present, or a family signer key has the wrong TOML shape.
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

    if let Some(keeta) = signers.get("keeta")
        && !matches!(keeta, toml::Value::String(_) | toml::Value::Table(_))
    {
        return Err(AppError::config(
            "[signers].keeta must be a seed string or a table { seed, indices }",
        ));
    }
    require_signer_string("tvm", signers.get("tvm"))?;
    require_signer_string_or_array("stellar", signers.get("stellar"))?;
    require_signer_string("stellar_fee_bump", signers.get("stellar_fee_bump"))?;

    let evm_signers = signers.get("evm").cloned().map(wrap_evm_signers);
    let tvm_signer = signers.get("tvm").cloned();
    let stellar = signers.get("stellar").cloned().map(wrap_as_array);
    let stellar_fee_bump = signers.get("stellar_fee_bump").cloned();
    let keeta = signers.get("keeta").cloned();
    let injections = [
        ("eip155:", "signers", evm_signers.as_ref()),
        ("tvm:", "signer", tvm_signer.as_ref()),
        ("stellar:", "signers", stellar.as_ref()),
        ("stellar:", "fee_bump", stellar_fee_bump.as_ref()),
    ];

    if let Some(toml::Value::Table(chains)) = doc.get_mut("chains") {
        inject_resolved_signers(chains, &injections);
        for (chain_id, chain_val) in chains.iter_mut() {
            let toml::Value::Table(chain_table) = chain_val else {
                continue;
            };
            if chain_id.starts_with("keeta:") {
                inject_keeta_seed_and_indices(chain_table, keeta.as_ref());
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
        assert!(preprocess_signers(&mut doc).is_err(), "xrpl forbidden");
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

    #[test]
    fn keeta_table_injects_seed_and_indices() {
        let toml_str = r#"
[signers]
keeta = { seed = "$KEETA_SEED", indices = [0, 1] }

[chains."keeta:1413829460"]
"#;
        let mut doc: BTreeMap<String, toml::Value> = toml::from_str(toml_str).unwrap();
        let seed = "00".repeat(32);
        let env = BTreeMap::from([("KEETA_SEED".to_owned(), seed.clone())]);
        preprocess_signers_with(&mut doc, mock_lookup(&env)).unwrap();
        let chains = doc.get("chains").and_then(toml::Value::as_table).unwrap();
        let chain = chains
            .get("keeta:1413829460")
            .and_then(toml::Value::as_table)
            .unwrap();
        assert_eq!(
            chain.get("seed").and_then(toml::Value::as_str),
            Some(seed.as_str()),
            "seed"
        );
        let indices = chain
            .get("indices")
            .and_then(toml::Value::as_array)
            .unwrap();
        assert_eq!(indices.len(), 2, "two indices");
    }

    #[test]
    fn keeta_string_injects_seed_only() {
        let toml_str = r#"
[signers]
keeta = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA="

[chains."keeta:1413829460"]
"#;
        let mut doc: BTreeMap<String, toml::Value> = toml::from_str(toml_str).unwrap();
        preprocess_signers(&mut doc).unwrap();
        let chains = doc.get("chains").and_then(toml::Value::as_table).unwrap();
        let chain = chains
            .get("keeta:1413829460")
            .and_then(toml::Value::as_table)
            .unwrap();
        assert!(chain.get("seed").is_some(), "seed");
        assert!(chain.get("indices").is_none(), "indices default later");
    }

    #[test]
    fn keeta_per_chain_seed_not_overridden() {
        let toml_str = r#"
[signers]
keeta = { seed = "global", indices = [9] }

[chains."keeta:1413829460"]
seed = "local"
indices = [0]
"#;
        let mut doc: BTreeMap<String, toml::Value> = toml::from_str(toml_str).unwrap();
        preprocess_signers(&mut doc).unwrap();
        let chains = doc.get("chains").and_then(toml::Value::as_table).unwrap();
        let chain = chains
            .get("keeta:1413829460")
            .and_then(toml::Value::as_table)
            .unwrap();
        assert_eq!(
            chain.get("seed").and_then(toml::Value::as_str),
            Some("local"),
            "per-chain wins"
        );
        let first = chain
            .get("indices")
            .and_then(toml::Value::as_array)
            .and_then(|a| a.first())
            .and_then(toml::Value::as_integer);
        assert_eq!(first, Some(0), "chain indices kept");
    }

    #[test]
    fn keeta_chain_indices_kept_when_seed_injected() {
        let toml_str = r#"
[signers]
keeta = { seed = "global", indices = [9] }

[chains."keeta:1413829460"]
indices = [0]
"#;
        let mut doc: BTreeMap<String, toml::Value> = toml::from_str(toml_str).unwrap();
        preprocess_signers(&mut doc).unwrap();
        let chains = doc.get("chains").and_then(toml::Value::as_table).unwrap();
        let chain = chains
            .get("keeta:1413829460")
            .and_then(toml::Value::as_table)
            .unwrap();
        assert_eq!(
            chain.get("seed").and_then(toml::Value::as_str),
            Some("global"),
            "seed injected"
        );
        let first = chain
            .get("indices")
            .and_then(toml::Value::as_array)
            .and_then(|a| a.first())
            .and_then(toml::Value::as_integer);
        assert_eq!(first, Some(0), "per-chain indices win");
    }

    #[test]
    fn keeta_invalid_shape_errors() {
        let toml_str = r#"
[signers]
keeta = ["not-a-table"]
"#;
        let mut doc: BTreeMap<String, toml::Value> = toml::from_str(toml_str).unwrap();
        let err = preprocess_signers(&mut doc).unwrap_err();
        assert!(err.to_string().contains("[signers].keeta"), "got {err}");
    }

    #[test]
    fn tvm_signer_injected() {
        let toml_str = r#"
[signers]
tvm = "$TVM_KEY"

[chains."tvm:-3"]
"#;
        let mut doc: BTreeMap<String, toml::Value> = toml::from_str(toml_str).unwrap();
        let key = "00".repeat(32);
        let env = BTreeMap::from([("TVM_KEY".to_owned(), key.clone())]);
        preprocess_signers_with(&mut doc, mock_lookup(&env)).unwrap();
        let chains = doc.get("chains").and_then(toml::Value::as_table).unwrap();
        let chain = chains
            .get("tvm:-3")
            .and_then(toml::Value::as_table)
            .unwrap();
        assert_eq!(
            chain.get("signer").and_then(toml::Value::as_str),
            Some(key.as_str()),
            "$VAR inject"
        );
    }

    #[test]
    fn stellar_array_and_fee_bump_injected() {
        let toml_str = r#"
[signers]
stellar = ["$STELLAR_SECRET"]
stellar_fee_bump = "$STELLAR_FEE_BUMP"

[chains."stellar:testnet"]
"#;
        let mut doc: BTreeMap<String, toml::Value> = toml::from_str(toml_str).unwrap();
        let env = BTreeMap::from([
            ("STELLAR_SECRET".to_owned(), "SSECRET".to_owned()),
            ("STELLAR_FEE_BUMP".to_owned(), "SBUMP".to_owned()),
        ]);
        preprocess_signers_with(&mut doc, mock_lookup(&env)).unwrap();
        let chains = doc.get("chains").and_then(toml::Value::as_table).unwrap();
        let chain = chains
            .get("stellar:testnet")
            .and_then(toml::Value::as_table)
            .unwrap();
        let signers = chain
            .get("signers")
            .and_then(toml::Value::as_array)
            .unwrap();
        assert_eq!(
            signers.first().and_then(toml::Value::as_str),
            Some("SSECRET"),
            "stellar secret"
        );
        assert_eq!(
            chain.get("fee_bump").and_then(toml::Value::as_str),
            Some("SBUMP"),
            "fee bump"
        );
    }

    #[test]
    fn lone_stellar_string_wrapped_to_array() {
        let toml_str = r#"
[signers]
stellar = "SSECRET"

[chains."stellar:testnet"]
"#;
        let mut doc: BTreeMap<String, toml::Value> = toml::from_str(toml_str).unwrap();
        preprocess_signers(&mut doc).unwrap();
        let chains = doc.get("chains").and_then(toml::Value::as_table).unwrap();
        let chain = chains
            .get("stellar:testnet")
            .and_then(toml::Value::as_table)
            .unwrap();
        let signers = chain
            .get("signers")
            .and_then(toml::Value::as_array)
            .unwrap();
        assert_eq!(signers.len(), 1, "wrapped");
    }

    #[test]
    fn stellar_per_chain_fee_bump_not_overridden() {
        let toml_str = r#"
[signers]
stellar = ["SGLOBAL"]
stellar_fee_bump = "SGLOBALBUMP"

[chains."stellar:testnet"]
signers = ["SLOCAL"]
fee_bump = "SLOCALBUMP"
"#;
        let mut doc: BTreeMap<String, toml::Value> = toml::from_str(toml_str).unwrap();
        preprocess_signers(&mut doc).unwrap();
        let chains = doc.get("chains").and_then(toml::Value::as_table).unwrap();
        let chain = chains
            .get("stellar:testnet")
            .and_then(toml::Value::as_table)
            .unwrap();
        assert_eq!(
            chain
                .get("signers")
                .and_then(toml::Value::as_array)
                .and_then(|a| a.first())
                .and_then(toml::Value::as_str),
            Some("SLOCAL"),
            "per-chain signers"
        );
        assert_eq!(
            chain.get("fee_bump").and_then(toml::Value::as_str),
            Some("SLOCALBUMP"),
            "per-chain fee_bump"
        );
    }

    #[test]
    fn tvm_non_string_signer_errors() {
        let toml_str = r#"
[signers]
tvm = { key = "nope" }
"#;
        let mut doc: BTreeMap<String, toml::Value> = toml::from_str(toml_str).unwrap();
        let err = preprocess_signers(&mut doc).unwrap_err();
        assert!(err.to_string().contains("[signers].tvm"), "got {err}");
    }

    #[test]
    fn stellar_non_string_or_array_errors() {
        let toml_str = r#"
[signers]
stellar = { secret = "SNOPE" }
"#;
        let mut doc: BTreeMap<String, toml::Value> = toml::from_str(toml_str).unwrap();
        let err = preprocess_signers(&mut doc).unwrap_err();
        assert!(err.to_string().contains("[signers].stellar"), "got {err}");
    }

    #[test]
    fn stellar_fee_bump_non_string_errors() {
        let toml_str = r#"
[signers]
stellar_fee_bump = ["SNOPE"]
"#;
        let mut doc: BTreeMap<String, toml::Value> = toml::from_str(toml_str).unwrap();
        let err = preprocess_signers(&mut doc).unwrap_err();
        assert!(
            err.to_string().contains("[signers].stellar_fee_bump"),
            "got {err}"
        );
    }
}
