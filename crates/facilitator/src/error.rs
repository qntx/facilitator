//! Process-level errors.

use thiserror::Error;

/// Boxed source for error chains.
type BoxError = Box<dyn std::error::Error + Send + Sync>;

/// Top-level error for config, secrets, and the HTTP process.
#[derive(Debug, Error)]
#[allow(
    clippy::error_impl_error,
    reason = "process-level error type is intentionally named Error"
)]
pub enum Error {
    /// Configuration could not be read, parsed, or validated.
    #[error("config: {context}")]
    Config {
        /// What went wrong.
        context: String,
        /// Underlying cause, if any.
        #[source]
        source: Option<BoxError>,
    },
    /// A secret source could not be resolved.
    #[error("secret: {context}")]
    Secret {
        /// What went wrong.
        context: String,
        /// Underlying cause, if any.
        #[source]
        source: Option<BoxError>,
    },
    /// Bind or serve failure.
    #[error("server: {context}")]
    Server {
        /// What went wrong.
        context: String,
        /// Underlying cause, if any.
        #[source]
        source: Option<BoxError>,
    },
}

impl Error {
    /// Config error without a source.
    pub(crate) fn config(context: impl Into<String>) -> Self {
        Self::Config {
            context: context.into(),
            source: None,
        }
    }

    /// Config error with a source.
    pub(crate) fn config_with(
        context: impl Into<String>,
        source: impl std::error::Error + Send + Sync + 'static,
    ) -> Self {
        Self::Config {
            context: context.into(),
            source: Some(Box::new(source)),
        }
    }

    /// Secret error without a source.
    pub(crate) fn secret(context: impl Into<String>) -> Self {
        Self::Secret {
            context: context.into(),
            source: None,
        }
    }

    /// Secret error with a source.
    pub(crate) fn secret_with(
        context: impl Into<String>,
        source: impl std::error::Error + Send + Sync + 'static,
    ) -> Self {
        Self::Secret {
            context: context.into(),
            source: Some(Box::new(source)),
        }
    }

    /// Server error with a source.
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
