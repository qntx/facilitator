//! x402 V2 facilitator HTTP process (r402 0.19).
//!
//! Composes in-process scheme facilitators and serves spec §7 routes.

#![allow(
    clippy::missing_docs_in_private_items,
    reason = "helpers are named; public API is documented"
)]

#[cfg(test)]
use http_body_util as _;
#[cfg(test)]
use serde_json as _;
#[cfg(test)]
use tower as _;

mod cli;
mod compose;
mod config;
mod error;
mod http;
mod secrets;
mod telemetry;

use std::io::Write;
use std::process::ExitCode;

use clap::Parser;
use cli::{Cli, Commands};
pub use compose::FacilitatorMap;
pub use config::{
    BuilderCodeToml, Config, EvmNetwork, EvmSchemeConfig, HttpAuth, HttpConfig, LogConfig,
    LogFormat, Network, RpcConfig, RpcEndpoint, SchemeTables, SvmExactConfig, SvmNetwork,
    SvmSchemeConfig, SvmUptoConfig, load_config, parse_config_toml,
};
pub use error::Error;
pub use http::{AppState, router};
pub use secrets::{KeyEncoding, SecretSource};

/// CLI entry: `init`, `validate`, or `serve`.
pub async fn run() -> ExitCode {
    let cli = Cli::parse();
    let result = dispatch(cli).await;
    if let Err(error) = result {
        print_error(&error);
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

async fn dispatch(cli: Cli) -> Result<(), Error> {
    match cli.command {
        Commands::Init { output, force } => cli::run_init(&output, force),
        Commands::Validate => {
            let config = load_config(&cli.config)?;
            cli::run_validate(&config)
        }
        Commands::Serve => {
            let config = load_config(&cli.config)?;
            cli::run_serve(&config).await
        }
    }
}

fn print_error(error: &Error) {
    let stderr = std::io::stderr();
    let mut handle = stderr.lock();
    write!(handle, "Error: {error}").ok();
    let mut source = std::error::Error::source(error);
    while let Some(cause) = source {
        write!(handle, ": {cause}").ok();
        source = std::error::Error::source(cause);
    }
    writeln!(handle).ok();
}
