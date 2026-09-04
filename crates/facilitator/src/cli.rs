//! clap: `init` | `validate` | `serve`.

use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use clap::{Parser, Subcommand};
use tokio::net::TcpListener;
use tokio::sync::watch;

use crate::config::Config;
use crate::error::Error;
use crate::http::{AppState, HttpTimeouts, router_from_config};
use crate::metrics;

/// Example written by `init`. Packaged with the crate (cargo publish tarball).
pub(crate) const EXAMPLE_CONFIG: &str =
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/config.example.toml"));

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
    /// Parse config, construct facilitators, print `/supported` JSON.
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
pub(crate) async fn run_validate(config: &Config) -> Result<(), Error> {
    crate::telemetry::init(&config.log);
    let lookup = |key: &str| std::env::var(key).ok();
    config.resolve_secrets(&lookup)?;
    let map = crate::compose::build(config, &lookup).await?;
    let supported = r402_facilitator::Facilitator::supported(&map)
        .await
        .map_err(|err| Error::config_with("supported aggregation failed", err))?;
    let mut out = std::io::stdout();
    serde_json::to_writer_pretty(&mut out, &supported)
        .map_err(|err| Error::config_with("failed to write /supported JSON", err))?;
    writeln!(out).map_err(|err| Error::config_with("failed to write /supported JSON", err))?;
    Ok(())
}

/// Bind the HTTP listener and serve protocol + ops routes.
pub(crate) async fn run_serve(config: &Config) -> Result<(), Error> {
    crate::telemetry::init(&config.log);
    let lookup = |key: &str| std::env::var(key).ok();
    config.resolve_secrets(&lookup)?;
    let app = router_from_config(app_state(config, &lookup).await?, &config.http)?;
    let drain = HttpTimeouts::from_http(&config.http).drain();
    let protocol = bind(config.http.listen).await?;
    let metrics = bind_metrics(config.http.metrics_listen).await?;
    log_listen(
        config.http.listen,
        metrics.as_ref().map(|(addr, ..)| *addr),
        drain,
    );
    serve_bound(protocol, app, metrics, drain).await
}

fn log_listen(
    protocol: std::net::SocketAddr,
    metrics: Option<std::net::SocketAddr>,
    drain: Duration,
) {
    // SettlementCache is moka; a second replica splits /settle.
    tracing::info!(
        listen = %protocol,
        drain_secs = drain.as_secs(),
        settlement_cache = "in-memory",
        pin_settle = true,
        "facilitator listening"
    );
    if let Some(addr) = metrics {
        tracing::info!(listen = %addr, "metrics listening");
    }
}

async fn serve_bound(
    protocol: TcpListener,
    app: Router,
    metrics: Option<(std::net::SocketAddr, TcpListener, Router)>,
    drain: Duration,
) -> Result<(), Error> {
    let shutdown = Shutdown::spawn();
    match metrics {
        Some((_, listener, metrics_app)) => {
            serve_protocol_and_metrics(protocol, app, listener, metrics_app, shutdown, drain).await
        }
        None => serve_one(protocol, app, shutdown.rx, drain).await,
    }
}

async fn bind_metrics(
    addr: Option<std::net::SocketAddr>,
) -> Result<Option<(std::net::SocketAddr, TcpListener, Router)>, Error> {
    let Some(addr) = addr else {
        return Ok(None);
    };
    let handle = metrics::install()?;
    let app = metrics::metrics_router(handle);
    let listener = bind(addr).await?;
    Ok(Some((addr, listener, app)))
}

async fn app_state(
    config: &Config,
    lookup: &(impl Fn(&str) -> Option<String> + Send + Sync),
) -> Result<AppState, Error> {
    let map = crate::compose::build(config, lookup).await?;
    let mut state = AppState::new(Arc::new(map)).with_ready(true);
    if let Some(token) = config.resolve_http_auth(lookup)? {
        state = state.with_bearer(token);
    }
    Ok(state)
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
    let (protocol_result, metrics_result) = tokio::join!(protocol_srv, metrics_srv);
    protocol_result?;
    metrics_result
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

#[derive(Clone, Debug)]
struct Shutdown {
    tx: watch::Sender<bool>,
    rx: watch::Receiver<bool>,
}

impl Shutdown {
    fn new() -> Self {
        let (tx, rx) = watch::channel(false);
        Self { tx, rx }
    }

    fn spawn() -> Self {
        let this = Self::new();
        let tx = this.tx.clone();
        tokio::spawn(async move {
            wait_shutdown_signal().await;
            tracing::info!("shutdown signal received");
            tx.send(true).ok();
        });
        this
    }

    #[cfg(test)]
    fn trigger(&self) {
        self.tx.send(true).ok();
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

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "unit tests"
)]
mod tests {
    use axum::routing::get;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    use super::*;

    async fn slow() -> &'static str {
        tokio::time::sleep(Duration::from_millis(80)).await;
        "ok"
    }

    async fn connect_retry(addr: std::net::SocketAddr) -> tokio::net::TcpStream {
        for _ in 0..50 {
            if let Ok(stream) = tokio::net::TcpStream::connect(addr).await {
                return stream;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        panic!("connect {addr}");
    }

    #[tokio::test]
    async fn metrics_idle_shutdown_does_not_drop_protocol() {
        let protocol_listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("protocol bind");
        let protocol_addr = protocol_listener.local_addr().expect("protocol addr");
        let metrics_listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("metrics bind");
        let protocol_app = Router::new().route("/slow", get(slow));
        let metrics_app = Router::new().route("/metrics", get(|| async { "" }));
        let shutdown = Shutdown::new();
        let server = tokio::spawn(serve_protocol_and_metrics(
            protocol_listener,
            protocol_app,
            metrics_listener,
            metrics_app,
            shutdown.clone(),
            Duration::from_secs(2),
        ));
        tokio::task::yield_now().await;

        let client = tokio::spawn(async move {
            let mut stream = connect_retry(protocol_addr).await;
            stream
                .write_all(b"GET /slow HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
                .await
                .expect("write");
            let mut buf = Vec::new();
            stream.read_to_end(&mut buf).await.expect("read");
            buf
        });
        tokio::time::sleep(Duration::from_millis(30)).await;
        shutdown.trigger();
        let body = client.await.expect("client join");
        let text = String::from_utf8_lossy(&body);
        assert!(
            text.contains("200"),
            "in-flight protocol request must drain after metrics idles, got {text}"
        );
        assert!(text.contains("ok"), "body, got {text}");
        server.await.expect("server join").expect("serve");
    }
}
