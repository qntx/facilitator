//! `facilitator serve` command — start the facilitator HTTP server.

use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;

use axum::Router;
use axum::extract::DefaultBodyLimit;
use axum::http::Method;
use dotenvy::dotenv;
use r402_core::facilitator::HookedFacilitator;
use r402_core::scheme::SchemeRegistry;
use tower_http::cors;

use crate::config::load_config;
use crate::error::AppError;
use crate::routes::{self, FacilitatorState};
use crate::telemetry::{Telemetry, TelemetryGuard};

/// Execute the `serve` command.
///
/// # Errors
///
/// Returns an error if configuration loading, provider initialisation,
/// or server binding fails.
pub(crate) async fn run(config_path: &Path) -> Result<(), AppError> {
    // Failure means a provider was already installed, which is acceptable.
    drop(rustls::crypto::CryptoProvider::install_default(
        rustls::crypto::ring::default_provider(),
    ));

    dotenv().ok();

    let config = load_config(config_path)?;

    let _guard = Telemetry::new()
        .with_name(env!("CARGO_PKG_NAME"))
        .with_version(env!("CARGO_PKG_VERSION"))
        .with_log_level(config.log_level())
        .register();

    let registry = build_registry(&config)?;
    let facilitator = HookedFacilitator::new(registry);
    let axum_state: FacilitatorState = Arc::new(facilitator);

    let http_endpoints = build_router(axum_state);

    let addr = SocketAddr::new(config.host(), config.port());
    tracing::info!("Starting server at http://{addr}");

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .inspect_err(|e| tracing::error!("Failed to bind to {addr}: {e}"))
        .map_err(|e| AppError::server_with("failed to bind", e))?;

    axum::serve(listener, http_endpoints)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .map_err(|e| AppError::server_with("server error", e))?;

    Ok(())
}

/// Construct chain handles and register compiled schemes.
#[cfg_attr(
    not(any(
        feature = "chain-eip155",
        feature = "chain-keeta",
        feature = "chain-tvm",
        feature = "chain-stellar"
    )),
    allow(unused_variables, reason = "no compiled chain families in this build")
)]
fn build_registry(config: &crate::config::Config) -> Result<SchemeRegistry, AppError> {
    let mut registry = SchemeRegistry::new();

    #[cfg(feature = "chain-eip155")]
    {
        use r402_evm::Eip155Exact;

        use crate::chain::eip155::build_eip155_handle;

        for chain in config.chains().eip155() {
            let handle = build_eip155_handle(chain)?;
            registry
                .register(&Eip155Exact, &handle, None)
                .map_err(|e| AppError::chain(format!("failed to register eip155 exact: {e}")))?;
        }
    }

    #[cfg(feature = "chain-keeta")]
    {
        use r402_keeta::KeetaExact;

        use crate::chain::keeta::build_keeta_provider;

        for chain in config.chains().keeta() {
            let provider = build_keeta_provider(chain)?;
            registry
                .register(&KeetaExact, &provider, None)
                .map_err(|e| AppError::chain(format!("failed to register keeta exact: {e}")))?;
        }
    }

    #[cfg(feature = "chain-tvm")]
    {
        use r402_tvm::TvmExact;

        use crate::chain::tvm::build_tvm_provider;

        for chain in config.chains().tvm() {
            let provider = build_tvm_provider(chain)?;
            registry
                .register(&TvmExact, &provider, chain.scheme_config_json())
                .map_err(|e| AppError::chain(format!("failed to register tvm exact: {e}")))?;
        }
    }

    #[cfg(feature = "chain-stellar")]
    {
        use r402_stellar::StellarExact;

        use crate::chain::stellar::build_stellar_provider;

        for chain in config.chains().stellar() {
            let provider = build_stellar_provider(chain)?;
            registry
                .register(&StellarExact, &provider, None)
                .map_err(|e| AppError::chain(format!("failed to register stellar exact: {e}")))?;
        }
    }

    Ok(registry)
}

/// Process middleware only. 30s/5s timeouts are inside `routes()` so health is not bound to the protocol budget.
fn build_router(state: FacilitatorState) -> Router {
    Router::new()
        .merge(routes::routes().with_state(state))
        .layer(TelemetryGuard::http_trace_layer())
        .layer(
            cors::CorsLayer::new()
                .allow_origin(cors::Any)
                .allow_methods([Method::GET, Method::POST])
                .allow_headers(cors::Any),
        )
        .layer(DefaultBodyLimit::max(64 * 1024))
}

/// Wait for a shutdown signal (Ctrl+C on all platforms, SIGTERM on Unix).
async fn shutdown_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal};
        match signal(SignalKind::terminate()) {
            Ok(mut sigterm) => {
                tokio::select! {
                    _ = tokio::signal::ctrl_c() => {}
                    _ = sigterm.recv() => {}
                }
            }
            Err(_) => drop(tokio::signal::ctrl_c().await),
        }
    }
    #[cfg(not(unix))]
    {
        drop(tokio::signal::ctrl_c().await);
    }
}
