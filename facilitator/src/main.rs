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
mod cmd;
mod config;
mod error;
mod routes;
mod signers;
#[cfg(feature = "telemetry")]
mod telemetry;

use clap::Parser;
use cmd::{Cli, Commands};
use error::Error;

#[tokio::main]
#[allow(clippy::print_stderr)]
async fn main() {
    let cli = Cli::parse();

    let result: Result<(), Error> = match cli.command {
        Commands::Init { output, force } => cmd::init::run(&output, force),
        Commands::Serve { config } => cmd::serve::run(&config).await,
    };

    if let Err(e) = result {
        eprintln!("Error: {e}");
        std::process::exit(1);
    }
}
