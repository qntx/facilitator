//! `facilitator serve` command — start the facilitator HTTP server.
//!
//! Reads TOML configuration, initialises chain providers and scheme handlers,
//! then starts an Axum HTTP server with graceful shutdown support.

use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::extract::DefaultBodyLimit;
use axum::http::{Method, StatusCode};
use dotenvy::dotenv;
use r402::chain::ChainProvider as ChainProviderTrait;
use r402::hooks::HookedFacilitator;
use r402::scheme::SchemeRegistry;
#[cfg(feature = "chain-eip155")]
use r402_evm::Eip155Exact;
#[cfg(feature = "chain-solana")]
use r402_svm::SolanaExact;
use tower_http::cors;
use tower_http::timeout::TimeoutLayer;

use crate::chain::build_chain_registry;
use crate::config::load_config;
use crate::error::Error;
use crate::routes::{self, FacilitatorState};
#[cfg(feature = "telemetry")]
use crate::telemetry::Telemetry;

/// Execute the `serve` command.
///
/// # Errors
///
/// Returns an error if configuration loading, provider initialisation,
/// or server binding fails.
///
/// # Panics
///
/// Panics if the rustls crypto provider cannot be installed.
#[allow(clippy::cognitive_complexity, clippy::future_not_send)]
pub async fn run(config_path: &Path) -> Result<(), Error> {
    // Initialize rustls crypto provider (ring)
    rustls::crypto::CryptoProvider::install_default(rustls::crypto::ring::default_provider())
        .expect("Failed to initialize rustls crypto provider");

    // Load .env variables
    dotenv().ok();

    #[cfg(feature = "telemetry")]
    let telemetry_guard = Telemetry::new()
        .with_name(env!("CARGO_PKG_NAME"))
        .with_version(env!("CARGO_PKG_VERSION"))
        .register();
    #[cfg(feature = "telemetry")]
    let telemetry_layer = telemetry_guard.http_tracing();

    let config = load_config(config_path)?;

    let chain_registry = build_chain_registry(config.chains()).await?;

    // Build scheme registry by registering blueprints for each configured scheme.
    #[allow(unused_mut)]
    let mut scheme_registry = SchemeRegistry::new();
    for scheme_entry in config.schemes() {
        let matching_providers = chain_registry.by_chain_id_pattern(&scheme_entry.chains);
        for provider in matching_providers {
            let chain_id = provider.chain_id();
            let namespace = chain_id.namespace();
            #[allow(unused_variables)]
            let result: Result<(), Box<dyn std::error::Error>> = match namespace {
                #[cfg(feature = "chain-eip155")]
                "eip155" => {
                    scheme_registry.register(&Eip155Exact, provider, scheme_entry.config.clone())
                }
                #[cfg(feature = "chain-solana")]
                "solana" => {
                    scheme_registry.register(&SolanaExact, provider, scheme_entry.config.clone())
                }
                _ => {
                    #[cfg(feature = "telemetry")]
                    tracing::warn!(
                        namespace,
                        chain = %chain_id,
                        scheme = %scheme_entry.id,
                        "Skipping unsupported namespace"
                    );
                    Ok(())
                }
            };
            #[allow(unreachable_code)]
            if let Err(e) = result {
                #[cfg(feature = "telemetry")]
                tracing::warn!(
                    chain = %chain_id,
                    scheme = %scheme_entry.id,
                    error = %e,
                    "Failed to register scheme handler"
                );
            }
        }
    }

    // Wrap with HookedFacilitator to enable lifecycle hooks.
    // SchemeRegistry implements Facilitator directly — no wrapper needed.
    let facilitator = HookedFacilitator::new(scheme_registry);

    let axum_state: FacilitatorState = Arc::new(facilitator);

    let http_endpoints = Router::new().merge(routes::routes().with_state(Arc::clone(&axum_state)));
    #[cfg(feature = "telemetry")]
    let http_endpoints = http_endpoints.layer(telemetry_layer);
    let http_endpoints = http_endpoints
        .layer(
            cors::CorsLayer::new()
                .allow_origin(cors::Any)
                .allow_methods([Method::GET, Method::POST])
                .allow_headers(cors::Any),
        )
        .layer(DefaultBodyLimit::max(64 * 1024))
        .layer(TimeoutLayer::with_status_code(
            StatusCode::GATEWAY_TIMEOUT,
            Duration::from_secs(45),
        ));

    let addr = SocketAddr::new(config.host(), config.port());
    #[cfg(feature = "telemetry")]
    tracing::info!("Starting server at http://{}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await;
    #[cfg(feature = "telemetry")]
    let listener = listener.inspect_err(|e| tracing::error!("Failed to bind to {}: {}", addr, e));
    let listener = listener.map_err(|e| Error::server_with("failed to bind", e))?;

    axum::serve(listener, http_endpoints)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .map_err(|e| Error::server_with("server error", e))?;

    Ok(())
}

/// Wait for a shutdown signal (Ctrl+C on all platforms, SIGTERM on Unix).
async fn shutdown_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal};
        let mut sigterm = signal(SignalKind::terminate()).expect("failed to register SIGTERM");
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {}
            _ = sigterm.recv() => {}
        }
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}
