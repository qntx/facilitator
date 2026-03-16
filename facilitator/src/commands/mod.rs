//! CLI definitions and command implementations for the facilitator.

use std::path::PathBuf;

use clap::{Parser, Subcommand};

pub mod init;
pub mod serve;

/// x402 Facilitator — payment verification and settlement server.
#[derive(Debug, Parser)]
#[command(name = "facilitator")]
#[command(version, about, long_about = None)]
pub struct Cli {
    /// Subcommand to execute.
    #[command(subcommand)]
    pub command: Commands,
}

/// Available subcommands.
#[derive(Debug, Subcommand)]
pub enum Commands {
    /// Generate a default TOML configuration file.
    Init {
        /// Output path for the configuration file.
        #[arg(short, long, default_value = "config.toml")]
        output: PathBuf,

        /// Overwrite the file if it already exists.
        #[arg(long, default_value_t = false)]
        force: bool,
    },

    /// Start the facilitator HTTP server.
    Serve {
        /// Path to the TOML configuration file.
        #[arg(short, long, env = "CONFIG", default_value = "config.toml")]
        config: PathBuf,
    },
}
