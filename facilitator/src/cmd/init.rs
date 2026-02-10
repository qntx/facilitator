//! `facilitator init` command — generate a default TOML configuration file.

use crate::config::generate_default_config;
use std::fs;
use std::path::Path;

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
pub fn run(output: &Path, force: bool) -> Result<(), Box<dyn std::error::Error>> {
    if output.exists() && !force {
        return Err(format!(
            "Config file '{}' already exists. Use --force to overwrite.",
            output.display()
        )
        .into());
    }

    let content = generate_default_config();
    fs::write(output, content)
        .map_err(|e| format!("Failed to write config file '{}': {e}", output.display()))?;

    eprintln!("Config file written to {}", output.display());
    Ok(())
}
