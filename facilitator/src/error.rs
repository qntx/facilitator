//! Unified error types for the facilitator.

use thiserror::Error;

/// Top-level error type for the facilitator application.
#[derive(Debug, Error)]
pub enum Error {
    /// Configuration file could not be resolved, read, or parsed.
    #[error("config: {0}")]
    Config(String),

    /// Signer key resolution or derivation failed.
    #[error("signer: {0}")]
    Signer(String),

    /// Chain provider initialization failed.
    #[error("chain: {0}")]
    Chain(String),

    /// Server bind or runtime error.
    #[error("server: {0}")]
    Server(String),
}
