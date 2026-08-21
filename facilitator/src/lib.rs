//! x402 V2 facilitator HTTP process.
//!
//! Composes r402 in-process scheme facilitators and serves spec §7 routes.

// Dev-dependencies are linked into the lib test target. With `chain-eip155`
// they are used by GET /supported unit tests; without it, silence unused_crate_dependencies.
#[cfg(all(test, not(feature = "metrics")))]
use ::metrics as _;
#[cfg(all(test, not(feature = "chain-eip155")))]
use http_body_util as _;
#[cfg(all(test, not(feature = "metrics")))]
use metrics_util as _;
#[cfg(all(test, not(feature = "chain-eip155")))]
use tower as _;

mod chain;
mod commands;
mod config;
mod error;
mod metrics;
mod routes;
mod signers;
mod telemetry;

use std::io::Write;

use clap::Parser;
use commands::{Cli, Commands};
use error::AppError;
pub use routes::{FacilitatorState, routes};

/// CLI entry: `init` or `serve`.
pub async fn run() -> std::process::ExitCode {
    let cli = Cli::parse();

    let result: Result<(), AppError> = match cli.command {
        Commands::Init { output, force } => commands::init::run(&output, force),
        Commands::Serve { config } => commands::serve::run(&config).await,
    };

    if let Err(ref e) = result {
        let stderr = std::io::stderr();
        let mut handle = stderr.lock();
        write!(handle, "Error: {e}").ok();
        let mut source = std::error::Error::source(e);
        while let Some(cause) = source {
            write!(handle, ": {cause}").ok();
            source = std::error::Error::source(cause);
        }
        writeln!(handle).ok();
        return std::process::ExitCode::FAILURE;
    }
    std::process::ExitCode::SUCCESS
}
