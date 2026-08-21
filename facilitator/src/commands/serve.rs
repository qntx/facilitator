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
        feature = "chain-near",
        feature = "chain-xrpl"
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

    #[cfg(feature = "chain-near")]
    {
        use r402_near::NearExact;

        use crate::chain::near::{build_near_provider, near_scheme_json};

        for chain in config.chains().near() {
            let provider = build_near_provider(chain)?;
            let json = near_scheme_json(&chain.inner);
            registry
                .register(&NearExact, &provider, json)
                .map_err(|e| AppError::chain(format!("failed to register near exact: {e}")))?;
        }
    }

    #[cfg(feature = "chain-xrpl")]
    {
        use r402_xrpl::XrplExact;

        use crate::chain::xrpl::build_xrpl_provider;

        for chain in config.chains().xrpl() {
            let provider = build_xrpl_provider(chain)?;
            registry
                .register(&XrplExact, &provider, None)
                .map_err(|e| AppError::chain(format!("failed to register xrpl exact: {e}")))?;
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
