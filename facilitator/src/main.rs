//! x402 Facilitator Server
//!
//! A production-ready HTTP server implementing the [x402](https://www.x402.org) payment protocol.
//!
//! This crate provides a complete, runnable facilitator that supports multiple blockchain
//! networks (EVM/EIP-155 and Solana) and can verify and settle payments on-chain.

mod chain;
mod config;
mod handlers;
mod local;
mod run;
mod schemes;
mod util;

use std::process;

#[tokio::main]
#[allow(clippy::print_stderr)]
async fn main() {
    if let Err(e) = run::run().await {
        eprintln!("{e}");
        process::exit(1);
    }
}
