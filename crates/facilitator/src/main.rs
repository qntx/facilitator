//! x402 facilitator binary.

#![allow(
    unused_crate_dependencies,
    reason = "the binary target only calls the library crate"
)]

#[tokio::main]
async fn main() -> std::process::ExitCode {
    facilitator::run().await
}
