//! `facilitator init` command — generate a default TOML configuration file.

use std::fs;
use std::path::Path;

use crate::config::generate_default_config;
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
