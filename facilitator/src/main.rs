//! x402 Facilitator Server
//!
//! A CLI tool and HTTP server implementing the [x402](https://www.x402.org)
//! payment protocol for multiple blockchain networks (EVM/EIP-155, Solana).
//!
//! ```sh
//! facilitator init            # Generate default config.toml
//! facilitator serve           # Start the server
//! ```

mod chain;
mod commands;
mod config;
mod error;
mod routes;
mod signers;
mod telemetry;

use std::io::Write;

use clap::Parser;
use commands::{Cli, Commands};
use error::AppError;

#[tokio::main]
async fn main() -> std::process::ExitCode {
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
