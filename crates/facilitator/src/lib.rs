//! x402 V2 facilitator HTTP process.
//!
//! Composes in-process scheme facilitators and serves spec §7 routes.

#![allow(
    clippy::missing_docs_in_private_items,
    reason = "helpers are named; public API is documented"
)]

mod cli;
mod compose;
mod config;
mod error;
mod http;
mod metrics;
mod secrets;
mod telemetry;

use std::io::Write;
use std::process::ExitCode;

use clap::Parser;
use cli::{Cli, Commands};
pub use compose::{FacilitatorMap, build};
#[cfg(feature = "aptos")]
pub use config::AptosNetwork;
#[cfg(feature = "avm")]
pub use config::AvmNetwork;
#[cfg(feature = "concordium")]
pub use config::ConcordiumAccount;
#[cfg(feature = "keeta")]
pub use config::KeetaNetwork;
#[cfg(any(feature = "near", feature = "hedera"))]
pub use config::NamedAccount;
#[cfg(feature = "near")]
pub use config::NearNetwork;
#[cfg(feature = "stellar")]
pub use config::StellarNetwork;
#[cfg(feature = "experimental-tron")]
pub use config::TronNetwork;
#[cfg(feature = "tvm")]
pub use config::TvmNetwork;
#[cfg(feature = "xrpl")]
pub use config::XrplNetwork;
pub use config::{
    BuilderCodeToml, Config, EvmNetwork, EvmSchemeConfig, HttpAuth, HttpConfig, LogConfig,
    LogFormat, Network, RpcConfig, RpcEndpoint, SchemeTables, SvmExactConfig, SvmNetwork,
    SvmSchemeConfig, SvmUptoConfig, load_config, parse_config_toml,
};
#[cfg(feature = "concordium")]
pub use config::{ConcordiumNetwork, GrpcConfig};
#[cfg(feature = "hedera")]
pub use config::{HederaAliasPolicy, HederaNetwork};
pub use error::Error;
pub use http::{AppState, HttpTimeouts, router, router_from_config, router_with_timeouts};
pub use metrics::{MetricsHandle, metrics_router};
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
            cli::run_validate(&config).await
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
