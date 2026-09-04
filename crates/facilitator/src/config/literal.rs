//! Reject private-key literals and obsolete 1.0.0 keys.

use crate::error::Error;

/// Walk a TOML document for forbidden keys and secret-shaped strings.
pub(crate) fn reject_literals_and_obsolete(value: &toml::Value) -> Result<(), Error> {
    reject_in_value(value)
}

fn reject_in_value(value: &toml::Value) -> Result<(), Error> {
    match value {
        toml::Value::String(s) => reject_literal_string(s),
        toml::Value::Array(items) => {
            for item in items {
                reject_in_value(item)?;
            }
            Ok(())
        }
        toml::Value::Table(table) => reject_in_table(table),
        toml::Value::Integer(_)
        | toml::Value::Float(_)
        | toml::Value::Boolean(_)
        | toml::Value::Datetime(_) => Ok(()),
    }
}

fn reject_in_table(table: &toml::map::Map<String, toml::Value>) -> Result<(), Error> {
    if table.contains_key("settlement_mode") {
        return Err(Error::config(
            "settlement_mode is not valid in a facilitator config",
        ));
    }
    for value in table.values() {
        reject_in_value(value)?;
    }
    Ok(())
}

/// Root keys from the 1.0.0 process that must not parse.
pub(crate) fn reject_obsolete_root(
    table: &toml::map::Map<String, toml::Value>,
) -> Result<(), Error> {
    if table.contains_key("schemes") {
        return Err(Error::config(
            "delete [[schemes]]; schemes are per-network lists",
        ));
    }
    if table.contains_key("signers") {
        return Err(Error::config(
            "delete [signers]; use named [signer.<id>] tables",
        ));
    }
    if table.contains_key("chains") {
        return Err(Error::config("delete [chains]; use [network.\"<caip2>\"]"));
    }
    if table.contains_key("host") || table.contains_key("port") {
        return Err(Error::config(
            "delete host/port; bind is http.listen (overlay FACILITATOR_HTTP_LISTEN)",
        ));
    }
    Ok(())
}

fn reject_literal_string(raw: &str) -> Result<(), Error> {
    if looks_like_private_key(raw) {
        return Err(Error::config(
            "private-key literal in TOML; use [signer.*] source = \"env\" or \"file\"",
        ));
    }
    Ok(())
}

/// Encoding-specific detector for secrets pasted into TOML strings.
fn looks_like_private_key(raw: &str) -> bool {
    let s = raw.trim();
    if s.is_empty() {
        return false;
    }
    hex_secp256k1(s)
        || s.starts_with("ed25519:")
        || stellar_secret(s)
        || hedera_der(s)
        || base58_keypair(s)
        || base64_seed(s)
}

fn hex_secp256k1(s: &str) -> bool {
    let hex = s
        .strip_prefix("0x")
        .or_else(|| s.strip_prefix("0X"))
        .unwrap_or(s);
    hex.len() == 64 && hex.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn stellar_secret(s: &str) -> bool {
    s.len() == 56
        && s.starts_with('S')
        && s.bytes()
            .all(|byte| matches!(byte, b'A'..=b'Z' | b'2'..=b'7'))
}

fn hedera_der(s: &str) -> bool {
    let lower = s.to_ascii_lowercase();
    (lower.contains("302e") || lower.contains("0x302e")) && s.len() > 40
}

fn base58_keypair(s: &str) -> bool {
    let len = s.len();
    (87..=88).contains(&len)
        && s.bytes().all(|byte| {
            matches!(byte, b'1'..=b'9' | b'A'..=b'H' | b'J'..=b'N' | b'P'..=b'Z' | b'a'..=b'k' | b'm'..=b'z')
        })
}

fn base64_seed(s: &str) -> bool {
    s.len() == 44
        && s.ends_with('=')
        && s.bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'/' | b'='))
}
