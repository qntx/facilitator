//! `facilitator serve` command — start the HTTP server.

use std::path::Path;

/// Execute the `serve` command.
///
/// Loads the TOML configuration from `config_path` and starts the
/// facilitator HTTP server.
///
/// # Errors
///
/// Returns an error if configuration loading, provider initialisation,
/// or server binding fails.
#[allow(clippy::future_not_send)]
pub async fn run(config_path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    crate::server::run(config_path).await
}
