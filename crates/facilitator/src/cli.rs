//! clap: `init` | `validate` | `serve`.

use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;

use clap::{Parser, Subcommand};

use crate::compose::FacilitatorMap;
use crate::config::Config;
use crate::error::Error;
use crate::http::{AppState, router};

/// Example written by `init`.
pub(crate) const EXAMPLE_CONFIG: &str = include_str!("../../../config.example.toml");

/// x402 facilitator process.
#[derive(Debug, Parser)]
#[command(name = "facilitator", version, about)]
pub(crate) struct Cli {
    /// Config path for `validate` and `serve`.
    #[arg(
        short,
        long,
        global = true,
        env = "FACILITATOR_CONFIG",
        default_value = "config.toml"
    )]
    pub config: PathBuf,
    /// Subcommand.
    #[command(subcommand)]
    pub command: Commands,
}

/// Subcommands.
#[derive(Debug, Subcommand)]
pub(crate) enum Commands {
    /// Write the example config to `PATH`.
    Init {
        /// Output path.
        #[arg(short, long, default_value = "config.toml")]
        output: PathBuf,
        /// Overwrite an existing file.
        #[arg(long, default_value_t = false)]
        force: bool,
    },
    /// Parse config, resolve secrets, print networks/schemes.
    Validate,
    /// Bind HTTP and serve `GET /supported`.
    Serve,
}

/// Run `init`.
pub(crate) fn run_init(output: &std::path::Path, force: bool) -> Result<(), Error> {
    if output.exists() && !force {
        return Err(Error::config(format!(
            "'{}' already exists, use --force to overwrite",
            output.display()
        )));
    }
    std::fs::write(output, EXAMPLE_CONFIG).map_err(|err| {
        Error::config_with(format!("failed to write '{}'", output.display()), err)
    })?;
    let stderr = std::io::stderr();
    let mut handle = stderr.lock();
    writeln!(handle, "Config file written to {}", output.display())
        .map_err(|err| Error::config_with("failed to write status", err))?;
    Ok(())
}

/// Run `validate`.
pub(crate) fn run_validate(config: &Config) -> Result<(), Error> {
    crate::telemetry::init(&config.log);
    config.resolve_secrets(&|key| std::env::var(key).ok())?;
    config
        .write_summary(std::io::stdout())
        .map_err(|err| Error::config_with("failed to write summary", err))?;
    Ok(())
}

/// Bind the HTTP listener and serve protocol routes.
pub(crate) async fn run_serve(config: &Config) -> Result<(), Error> {
    crate::telemetry::init(&config.log);
    config.resolve_secrets(&|key| std::env::var(key).ok())?;
    let state = AppState::new(Arc::new(FacilitatorMap::new()));
    let app = router(state);
    let listener = tokio::net::TcpListener::bind(config.http.listen)
        .await
        .map_err(|err| Error::server_with(format!("failed to bind {}", config.http.listen), err))?;
    tracing::info!(listen = %config.http.listen, "facilitator listening");
    axum::serve(listener, app)
        .await
        .map_err(|err| Error::server_with("server error", err))?;
    Ok(())
}
