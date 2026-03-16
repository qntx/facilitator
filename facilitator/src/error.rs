//! Unified error types for the facilitator.

use thiserror::Error;

/// Boxed, thread-safe error used as an opaque source in error chains.
type BoxError = Box<dyn std::error::Error + Send + Sync>;

/// Top-level error type for the facilitator application.
///
/// Each variant carries a human-readable `context` message and an optional
/// `source` error that preserves the original cause for `Error::source()`
/// chain traversal and structured logging.
#[derive(Debug, Error)]
pub enum Error {
    /// Configuration file could not be resolved, read, or parsed.
    #[error("config: {context}")]
    Config {
        /// What went wrong, in plain English.
        context: String,
        /// The underlying error, if any.
        #[source]
        source: Option<BoxError>,
    },

    /// Signer key resolution or derivation failed.
    #[error("signer: {context}")]
    Signer {
        /// What went wrong, in plain English.
        context: String,
        /// The underlying error, if any.
        #[source]
        source: Option<BoxError>,
    },

    /// Chain provider initialisation failed.
    #[error("chain: {context}")]
    Chain {
        /// What went wrong, in plain English.
        context: String,
        /// The underlying error, if any.
        #[source]
        source: Option<BoxError>,
    },

    /// Server bind or runtime error.
    #[error("server: {context}")]
    Server {
        /// What went wrong, in plain English.
        context: String,
        /// The underlying error, if any.
        #[source]
        source: Option<BoxError>,
    },
}

impl Error {
    /// Create a config error with context only (no underlying cause).
    pub(crate) fn config(context: impl Into<String>) -> Self {
        Self::Config {
            context: context.into(),
            source: None,
        }
    }

    /// Create a config error with context and an underlying cause.
    pub(crate) fn config_with(
        context: impl Into<String>,
        source: impl std::error::Error + Send + Sync + 'static,
    ) -> Self {
        Self::Config {
            context: context.into(),
            source: Some(Box::new(source)),
        }
    }

    /// Create a signer error with context only.
    pub(crate) fn signer(context: impl Into<String>) -> Self {
        Self::Signer {
            context: context.into(),
            source: None,
        }
    }

    /// Create a chain error with context only.
    pub(crate) fn chain(context: impl Into<String>) -> Self {
        Self::Chain {
            context: context.into(),
            source: None,
        }
    }

    /// Create a chain error with context and an underlying cause.
    pub(crate) fn chain_with(
        context: impl Into<String>,
        source: impl std::error::Error + Send + Sync + 'static,
    ) -> Self {
        Self::Chain {
            context: context.into(),
            source: Some(Box::new(source)),
        }
    }

    /// Create a server error with an underlying cause.
    pub(crate) fn server_with(
        context: impl Into<String>,
        source: impl std::error::Error + Send + Sync + 'static,
    ) -> Self {
        Self::Server {
            context: context.into(),
            source: Some(Box::new(source)),
        }
    }
}
