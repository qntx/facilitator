//! `facilitator init` command — generate a default TOML configuration file.

use std::fs;
use std::path::Path;

use crate::error::AppError;

/// Execute the `init` command.
///
/// Writes a default TOML configuration template to `output`. Refuses to
/// overwrite an existing file unless `force` is `true`.
///
/// # Errors
///
/// Returns an error if the file already exists (without `--force`) or if
/// writing fails.
pub(crate) fn run(output: &Path, force: bool) -> Result<(), AppError> {
    if output.exists() && !force {
        return Err(AppError::config(format!(
            "'{}' already exists, use --force to overwrite",
            output.display()
        )));
    }

    let content = generate_default_config();
    fs::write(output, content)
        .map_err(|e| AppError::config_with(format!("failed to write '{}'", output.display()), e))?;

    let stderr = std::io::stderr();
    let mut handle = stderr.lock();
    std::io::Write::write_fmt(
        &mut handle,
        format_args!("Config file written to {}\n", output.display()),
    )
    .ok();
    Ok(())
}

/// Generate a default TOML configuration template for compiled families.
#[must_use]
fn generate_default_config() -> String {
    let mut config = String::from(
        r#"# x402 Facilitator Configuration
# https://www.x402.org

# Server bind address and port.
# Can also be set via HOST / PORT environment variables.
host = "0.0.0.0"
port = 8080

# Log level filter (RUST_LOG env var takes precedence when set).
# Examples: "info", "debug", "facilitator=debug,r402=trace"
log_level = "info"

# Global Signers
#
# Shared across all chains of the same type.
# Per-chain overrides are still possible (add `signers` to the
# individual chain table).
#
# Use environment variable references ($VAR or ${VAR}) for secrets.

[signers]
"#,
    );

    #[cfg(feature = "chain-eip155")]
    config.push_str(
        r#"evm = ["$EVM_SIGNER_PRIVATE_KEY"]       # hex, 0x-prefixed
"#,
    );

    #[cfg(feature = "chain-keeta")]
    config.push_str(
        r#"keeta = { seed = "$KEETA_SEED", indices = [0] }  # hex or standard-base64 32-byte seed
"#,
    );

    #[cfg(feature = "chain-tvm")]
    config.push_str(
        r#"tvm = "$TVM_SIGNER_PRIVATE_KEY"         # hex or base64 32- or 64-byte
"#,
    );

    #[cfg(feature = "chain-stellar")]
    config.push_str(
        r#"stellar = ["$STELLAR_SECRET_SEED"]     # S… secret
"#,
    );

    #[cfg(feature = "chain-eip155")]
    config.push_str(
        r#"
# EIP-155 (EVM) chains
#
# Key format: "eip155:<chain_id>"
# Only RPC config is needed; signers are injected from [signers] above.
# Schemes are compiled in (exact); do not add [[schemes]].

[chains."eip155:84532"]
rpc = [{ http = "https://sepolia.base.org" }]
receipt_timeout_secs = 20
"#,
    );

    #[cfg(feature = "chain-keeta")]
    config.push_str(
        r#"
# Keeta
#
# Key format: "keeta:21378" | "keeta:1413829460"
# No `rpc`. Seed is hex or standard-base64 32 bytes; mnemonics are not supported.

[chains."keeta:1413829460"]
"#,
    );

    #[cfg(feature = "chain-tvm")]
    config.push_str(
        r#"
# TON (TVM)
#
# Key format: "tvm:-239" | "tvm:-3"
# rpc / provider_base_url is an optional string URL. Keep confirmation_timeout_seconds ≤ 20
# unless the 30 s HTTP client timeout is raised with it.

[chains."tvm:-3"]
"#,
    );

    #[cfg(feature = "chain-stellar")]
    config.push_str(
        r#"
# Stellar
#
# Key format: "stellar:pubnet" | "stellar:testnet"
# rpc is an optional string URL; pubnet requires a non-empty rpc.
# Optional fee_bump / [signers].stellar_fee_bump, horizon_url, max_transaction_fee_stroops.

[chains."stellar:testnet"]
"#,
    );

    config
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;

    #[test]
    fn generate_default_config_is_valid_toml() {
        let config_str = generate_default_config();
        let parsed: Result<BTreeMap<String, toml::Value>, _> = toml::from_str(&config_str);
        assert!(parsed.is_ok(), "Generated config must be valid TOML");
    }

    #[test]
    fn generate_default_config_has_required_fields() {
        let config_str = generate_default_config();
        let doc: BTreeMap<String, toml::Value> = toml::from_str(&config_str).unwrap();
        assert!(doc.contains_key("host"), "host");
        assert!(doc.contains_key("port"), "port");
        assert!(doc.contains_key("signers"), "signers");
        assert!(!doc.contains_key("schemes"), "no [[schemes]]");
    }

    #[cfg(feature = "chain-eip155")]
    #[test]
    fn generate_default_config_has_eip155_chain() {
        let config_str = generate_default_config();
        let doc: BTreeMap<String, toml::Value> = toml::from_str(&config_str).unwrap();
        let chains = doc.get("chains").and_then(toml::Value::as_table).unwrap();
        assert!(chains.contains_key("eip155:84532"), "base sepolia present");
    }

    #[cfg(feature = "chain-keeta")]
    #[test]
    fn generate_default_config_has_keeta_chain() {
        let config_str = generate_default_config();
        let doc: BTreeMap<String, toml::Value> = toml::from_str(&config_str).unwrap();
        let chains = doc.get("chains").and_then(toml::Value::as_table).unwrap();
        assert!(chains.contains_key("keeta:1413829460"), "keeta testnet");
    }

    #[cfg(feature = "chain-tvm")]
    #[test]
    fn generate_default_config_has_tvm_chain() {
        let config_str = generate_default_config();
        let doc: BTreeMap<String, toml::Value> = toml::from_str(&config_str).unwrap();
        let chains = doc.get("chains").and_then(toml::Value::as_table).unwrap();
        assert!(chains.contains_key("tvm:-3"), "tvm testnet");
    }

    #[cfg(feature = "chain-stellar")]
    #[test]
    fn generate_default_config_has_stellar_chain() {
        let config_str = generate_default_config();
        let doc: BTreeMap<String, toml::Value> = toml::from_str(&config_str).unwrap();
        let chains = doc.get("chains").and_then(toml::Value::as_table).unwrap();
        assert!(chains.contains_key("stellar:testnet"), "stellar testnet");
    }
}
