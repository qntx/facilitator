//! `facilitator serve` command — start the facilitator HTTP server.
//!
//! Reads TOML configuration, initialises chain providers and scheme handlers,
//! then starts an Axum HTTP server with graceful shutdown support.

use axum::Router;
use axum::http::Method;
use dotenvy::dotenv;
use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;
use tower_http::cors;

use crate::config::load_config;
use crate::facilitator::FacilitatorLocal;
use crate::routes;
use crate::signal::SigDown;

use r402::chain::ChainRegistry;
use r402::chain::FromConfig;
use r402::scheme::{SchemeBlueprints, SchemeRegistry};

#[cfg(feature = "telemetry")]
use crate::telemetry::Telemetry;
#[cfg(feature = "chain-eip155")]
use r402_evm::{V1Eip155Exact, V2Eip155Exact};
#[cfg(feature = "chain-solana")]
use r402_svm::{V1SolanaExact, V2SolanaExact};

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
pub async fn run(config_path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    // Initialize rustls crypto provider (ring)
    rustls::crypto::CryptoProvider::install_default(rustls::crypto::ring::default_provider())
        .expect("Failed to initialize rustls crypto provider");

    // Load .env variables
    dotenv().ok();

    #[cfg(feature = "telemetry")]
    let telemetry_layer = {
        let telemetry = Telemetry::new()
            .with_name(env!("CARGO_PKG_NAME"))
            .with_version(env!("CARGO_PKG_VERSION"))
            .register();
        telemetry.http_tracing()
    };

    let config = load_config(config_path)?;

    let chain_registry = ChainRegistry::from_config(config.chains()).await?;
    let scheme_blueprints = {
        #[allow(unused_mut)]
        let mut scheme_blueprints = SchemeBlueprints::new();
        #[cfg(feature = "chain-eip155")]
        {
            scheme_blueprints.register(V1Eip155Exact);
            scheme_blueprints.register(V2Eip155Exact);
        }
        #[cfg(feature = "chain-solana")]
        {
            scheme_blueprints.register(V1SolanaExact);
            scheme_blueprints.register(V2SolanaExact);
        }
        scheme_blueprints
    };
    let scheme_registry =
        SchemeRegistry::build(&chain_registry, &scheme_blueprints, config.schemes());

    let facilitator = FacilitatorLocal::new(scheme_registry);
    let axum_state = Arc::new(facilitator);

    let http_endpoints = Router::new().merge(routes::routes().with_state(axum_state));
    #[cfg(feature = "telemetry")]
    let http_endpoints = http_endpoints.layer(telemetry_layer);
    let http_endpoints = http_endpoints.layer(
        cors::CorsLayer::new()
            .allow_origin(cors::Any)
            .allow_methods([Method::GET, Method::POST])
            .allow_headers(cors::Any),
    );

    let addr = SocketAddr::new(config.host(), config.port());
    #[cfg(feature = "telemetry")]
    tracing::info!("Starting server at http://{}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await;
    #[cfg(feature = "telemetry")]
    let listener = listener.inspect_err(|e| tracing::error!("Failed to bind to {}: {}", addr, e));
    let listener = listener?;

    let sig_down = SigDown::try_new()?;
    let axum_cancellation_token = sig_down.cancellation_token();
    let axum_graceful_shutdown = async move { axum_cancellation_token.cancelled().await };
    axum::serve(listener, http_endpoints)
        .with_graceful_shutdown(axum_graceful_shutdown)
        .await?;

    Ok(())
}
