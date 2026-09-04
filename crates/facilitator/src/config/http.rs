//! `[http]`, `[http.auth]`, and `[log]` tables.

use std::net::{Ipv4Addr, SocketAddr};
use std::time::Duration;

use serde::Deserialize;

/// Protocol bind and HTTP process knobs.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HttpConfig {
    /// Protocol listen address.
    #[serde(default = "default_listen")]
    pub listen: SocketAddr,
    /// Timeout for `POST /verify`.
    #[serde(default = "default_protocol_timeout", with = "humantime_serde")]
    pub verify_timeout: Duration,
    /// Timeout for `POST /settle`.
    #[serde(default = "default_protocol_timeout", with = "humantime_serde")]
    pub settle_timeout: Duration,
    /// Maximum request body size in bytes.
    #[serde(default = "default_body_limit")]
    pub body_limit_bytes: u64,
    /// Optional Prometheus listen address. Omitted or null: no recorder.
    #[serde(default)]
    pub metrics_listen: Option<SocketAddr>,
    /// CORS allowlist. Empty: no CORS layer.
    #[serde(default)]
    pub cors_origins: Vec<String>,
    /// Optional shared bearer on protocol routes.
    #[serde(default)]
    pub auth: Option<HttpAuth>,
}

impl Default for HttpConfig {
    fn default() -> Self {
        Self {
            listen: default_listen(),
            verify_timeout: default_protocol_timeout(),
            settle_timeout: default_protocol_timeout(),
            body_limit_bytes: default_body_limit(),
            metrics_listen: None,
            cors_origins: Vec::new(),
            auth: None,
        }
    }
}

/// `Authorization: Bearer` env name. Presence of this table is enforced later.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HttpAuth {
    /// Environment variable holding the shared token.
    pub bearer_env: String,
}

/// Console log settings.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LogConfig {
    /// Filter when `RUST_LOG` is unset.
    #[serde(default = "default_log_level")]
    pub level: String,
    /// Console formatter.
    #[serde(default)]
    pub format: LogFormat,
}

impl Default for LogConfig {
    fn default() -> Self {
        Self {
            level: default_log_level(),
            format: LogFormat::Json,
        }
    }
}

/// `tracing-subscriber` formatter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogFormat {
    /// JSON logs (default).
    #[default]
    Json,
    /// Compact text logs.
    Compact,
}

fn default_listen() -> SocketAddr {
    SocketAddr::from((Ipv4Addr::LOCALHOST, 8080))
}

const fn default_protocol_timeout() -> Duration {
    Duration::from_secs(30)
}

const fn default_body_limit() -> u64 {
    262_144
}

fn default_log_level() -> String {
    "info".to_owned()
}
