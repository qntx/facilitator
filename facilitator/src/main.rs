//! x402 Facilitator Server
//!
//! HTTP process implementing the [x402](https://www.x402.org) V2 facilitator
//! surface (`POST /verify`, `POST /settle`, `GET /supported`) plus process
//! health.
//!
//! ```sh
//! facilitator init            # Generate default config.toml
//! facilitator serve           # Start the server
//! ```

#![allow(
    unused_crate_dependencies,
    reason = "the binary target only calls the library crate"
)]

#[tokio::main]
async fn main() -> std::process::ExitCode {
    facilitator::run().await
}
