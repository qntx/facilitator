//! `facilitator init` command — generate a default TOML configuration file.

use std::fs;
use std::path::Path;

use crate::error::Error;

/// Execute the `init` command.
///
/// Writes a default TOML configuration template to `output`. Refuses to
/// overwrite an existing file unless `force` is `true`.
///
/// # Errors
///
/// Returns an error if the file already exists (without `--force`) or if
/// writing fails.
#[allow(clippy::print_stderr)]
pub fn run(output: &Path, force: bool) -> Result<(), Error> {
    if output.exists() && !force {
        return Err(Error::config(format!(
            "'{}' already exists, use --force to overwrite",
            output.display()
        )));
    }

    let content = generate_default_config();
    fs::write(output, content)
        .map_err(|e| Error::config_with(format!("failed to write '{}'", output.display()), e))?;

    eprintln!("Config file written to {}", output.display());
    Ok(())
}

/// Generate a default TOML configuration template.
///
/// The output includes commented sections for every chain family enabled
/// at compile time.  Uses the simplified format with global signers
/// and environment variable resolution.
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
# Per-chain overrides are still possible (add `signers` / `signer` to
# the individual chain table).
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

    #[cfg(feature = "chain-solana")]
    config.push_str(
        r#"solana = "$SOLANA_SIGNER_PRIVATE_KEY"    # base58, 64-byte keypair
"#,
    );

    #[cfg(feature = "chain-eip155")]
    config.push_str(
        r#"
# EIP-155 (EVM) chains
#
# Key format: "eip155:<chain_id>"
# Only RPC config is needed; signers are injected from [signers] above.

[chains."eip155:84532"]
rpc = [{ http = "https://sepolia.base.org" }]
"#,
    );

    #[cfg(feature = "chain-solana")]
    config.push_str(
        r#"
# Solana chains
#
# Key format: "solana:<genesis_hash>"

[chains."solana:EtWTRABZaYq6iMfeYKouRu166VU2xqa1"]
rpc = "https://api.devnet.solana.com"
"#,
    );

    config.push_str(
        r#"
# Scheme registrations (optional)
#
# If omitted, all configured chains are auto-registered with
# every available scheme.
#
# Uncomment below only if you need to restrict schemes:
#
# [[schemes]]
# id = "eip155-exact"
# chains = "eip155:84532"
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
        assert!(doc.contains_key("host"));
        assert!(doc.contains_key("port"));
        assert!(doc.contains_key("signers"));
    }
}
