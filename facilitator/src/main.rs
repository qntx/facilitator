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

use clap::Parser;
use commands::{Cli, Commands};
use error::Error;

#[tokio::main]
#[allow(clippy::print_stderr)]
async fn main() {
    let cli = Cli::parse();

    let result: Result<(), Error> = match cli.command {
        Commands::Init { output, force } => commands::init::run(&output, force),
        Commands::Serve { config } => commands::serve::run(&config).await,
    };

    if let Err(ref e) = result {
        eprint!("Error: {e}");
        // Walk the source chain so structured causes are not lost.
        let mut source = std::error::Error::source(e);
        while let Some(cause) = source {
            eprint!(": {cause}");
            source = std::error::Error::source(cause);
        }
        eprintln!();
        std::process::exit(1);
    }
}
