//! clap: `init` | `validate` | `serve`.

use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use clap::{Parser, Subcommand};
use tokio::net::TcpListener;
use tokio::sync::watch;

use crate::compose::FacilitatorMap;
use crate::config::Config;
use crate::error::Error;
use crate::http::{AppState, router_from_config};
use crate::metrics;

/// Slack added to `settle_timeout` for SIGTERM drain.
const DRAIN_SLACK: Duration = Duration::from_secs(5);

/// Example written by `init`.
pub(crate) const EXAMPLE_CONFIG: &str = include_str!("../../../config.example.toml");

/// x402 facilitator process.
#[derive(Debug, Parser)]
#[command(name = "facilitator", version, about)]
pub(crate) struct Cli {
    /// Config path for `validate` and `serve`.
    #[arg(
        short,
        long,
        global = true,
        env = "FACILITATOR_CONFIG",
        default_value = "config.toml"
    )]
    pub config: PathBuf,
    /// Subcommand.
    #[command(subcommand)]
    pub command: Commands,
}

/// Subcommands.
#[derive(Debug, Subcommand)]
pub(crate) enum Commands {
    /// Write the example config to `PATH`.
    Init {
        /// Output path.
        #[arg(short, long, default_value = "config.toml")]
        output: PathBuf,
        /// Overwrite an existing file.
        #[arg(long, default_value_t = false)]
        force: bool,
    },
    /// Parse config, resolve secrets, print networks/schemes.
    Validate,
    /// Bind HTTP and serve protocol + ops routes.
    Serve,
}

/// Run `init`.
pub(crate) fn run_init(output: &std::path::Path, force: bool) -> Result<(), Error> {
    if output.exists() && !force {
        return Err(Error::config(format!(
            "'{}' already exists, use --force to overwrite",
            output.display()
        )));
    }
    std::fs::write(output, EXAMPLE_CONFIG).map_err(|err| {
        Error::config_with(format!("failed to write '{}'", output.display()), err)
    })?;
    let stderr = std::io::stderr();
    let mut handle = stderr.lock();
    writeln!(handle, "Config file written to {}", output.display())
        .map_err(|err| Error::config_with("failed to write status", err))?;
    Ok(())
}

/// Run `validate`.
pub(crate) fn run_validate(config: &Config) -> Result<(), Error> {
    crate::telemetry::init(&config.log);
    config.resolve_secrets(&|key| std::env::var(key).ok())?;
    config
        .write_summary(std::io::stdout())
        .map_err(|err| Error::config_with("failed to write summary", err))?;
    Ok(())
}

/// Bind the HTTP listener and serve protocol + ops routes.
pub(crate) async fn run_serve(config: &Config) -> Result<(), Error> {
    crate::telemetry::init(&config.log);
    let lookup = |key: &str| std::env::var(key).ok();
    config.resolve_secrets(&lookup)?;
    let app = router_from_config(app_state(config, &lookup)?, &config.http)?;
    let drain = config.http.settle_timeout.saturating_add(DRAIN_SLACK);
    let protocol = bind(config.http.listen).await?;
    tracing::info!(
        listen = %config.http.listen,
        drain_secs = drain.as_secs(),
        "facilitator listening"
    );
    serve_listeners(protocol, app, config.http.metrics_listen, drain).await
}

fn app_state(config: &Config, lookup: &impl Fn(&str) -> Option<String>) -> Result<AppState, Error> {
    let map = FacilitatorMap::new();
    let ready = !map.is_empty();
    let mut state = AppState::new(Arc::new(map)).with_ready(ready);
    if let Some(token) = config.resolve_http_auth(lookup)? {
        state = state.with_bearer(token);
    }
    Ok(state)
}

async fn serve_listeners(
    protocol: TcpListener,
    app: Router,
    metrics_listen: Option<std::net::SocketAddr>,
    drain: Duration,
) -> Result<(), Error> {
    let shutdown = Shutdown::spawn();
    let Some(addr) = metrics_listen else {
        return serve_one(protocol, app, shutdown.rx, drain).await;
    };
    let handle = metrics::install()?;
    let metrics_app = metrics::metrics_router(handle);
    let metrics_listener = bind(addr).await?;
    tracing::info!(listen = %addr, "metrics listening");
    serve_protocol_and_metrics(
        protocol,
        app,
        metrics_listener,
        metrics_app,
        shutdown,
        drain,
    )
    .await
}

async fn serve_protocol_and_metrics(
    protocol: TcpListener,
    app: Router,
    metrics_listener: TcpListener,
    metrics_app: Router,
    shutdown: Shutdown,
    drain: Duration,
) -> Result<(), Error> {
    let protocol_srv = serve_one(protocol, app, shutdown.rx.clone(), drain);
    let metrics_srv = serve_one(metrics_listener, metrics_app, shutdown.rx, drain);
    tokio::select! {
        result = protocol_srv => result,
        result = metrics_srv => result,
    }
}

async fn bind(addr: std::net::SocketAddr) -> Result<TcpListener, Error> {
    TcpListener::bind(addr)
        .await
        .map_err(|err| Error::server_with(format!("failed to bind {addr}"), err))
}

async fn serve_one(
    listener: TcpListener,
    app: Router,
    mut rx: watch::Receiver<bool>,
    drain: Duration,
) -> Result<(), Error> {
    let mut deadline_rx = rx.clone();
    let server = axum::serve(listener, app).with_graceful_shutdown(async move {
        rx.wait_for(|stop| *stop).await.ok();
    });
    tokio::select! {
        result = server => result.map_err(|err| Error::server_with("server error", err)),
        () = drain_after_signal(&mut deadline_rx, drain) => {
            tracing::warn!(?drain, "drain deadline exceeded");
            Ok(())
        }
    }
}

async fn drain_after_signal(rx: &mut watch::Receiver<bool>, drain: Duration) {
    rx.wait_for(|stop| *stop).await.ok();
    tokio::time::sleep(drain).await;
}

#[derive(Debug)]
struct Shutdown {
    rx: watch::Receiver<bool>,
}

impl Shutdown {
    fn spawn() -> Self {
        let (tx, rx) = watch::channel(false);
        tokio::spawn(async move {
            wait_shutdown_signal().await;
            tracing::info!("shutdown signal received");
            tx.send(true).ok();
        });
        Self { rx }
    }
}

async fn wait_shutdown_signal() {
    let ctrl_c = async {
        if let Err(error) = tokio::signal::ctrl_c().await {
            tracing::error!(?error, "ctrl_c handler failed");
        }
    };
    #[cfg(unix)]
    {
        let mut sigterm =
            match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
                Ok(signal) => signal,
                Err(error) => {
                    tracing::error!(?error, "SIGTERM handler failed");
                    ctrl_c.await;
                    return;
                }
            };
        tokio::select! {
            () = ctrl_c => {}
            _ = sigterm.recv() => {}
        }
    }
    #[cfg(not(unix))]
    {
        ctrl_c.await;
    }
}
