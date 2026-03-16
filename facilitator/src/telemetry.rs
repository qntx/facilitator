//! Observability: structured logging and optional `OpenTelemetry` export.
//!
//! [`Telemetry`] always initialises `tracing-subscriber` for structured console
//! logging. When the `telemetry` feature is enabled **and** `OTEL_EXPORTER_OTLP_*`
//! environment variables are present, it additionally registers `OpenTelemetry`
//! trace and metrics exporters via OTLP.

use std::time::Duration;

use axum::http::{Request, Response};
#[cfg(feature = "telemetry")]
use opentelemetry::trace::TracerProvider;
#[cfg(feature = "telemetry")]
use opentelemetry::{KeyValue, global};
#[cfg(feature = "telemetry")]
use opentelemetry_sdk::{
    Resource,
    metrics::{MeterProviderBuilder, PeriodicReader, SdkMeterProvider},
    trace::SdkTracerProvider,
};
#[cfg(feature = "telemetry")]
use opentelemetry_semantic_conventions::{
    SCHEMA_URL,
    attribute::{DEPLOYMENT_ENVIRONMENT_NAME, SERVICE_VERSION},
};
use tower_http::trace::{MakeSpan, OnResponse, TraceLayer};
use tracing::Span;
#[cfg(feature = "telemetry")]
use tracing_opentelemetry::{MetricsLayer, OpenTelemetryLayer};
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

/// Observability configuration.
///
/// Always initialises structured console logging. When the `telemetry` feature
/// is active and OTLP env vars are set, also registers `OpenTelemetry` exporters.
#[derive(Debug, Default)]
pub struct Telemetry {
    name: Option<String>,
    version: Option<String>,
    log_level: Option<String>,
}

impl Telemetry {
    /// Creates a new, empty [`Telemetry`] instance.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the service name (used for `OTel` resource identification).
    #[must_use]
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    /// Sets the service version (used for `OTel` resource identification).
    #[must_use]
    pub fn with_version(mut self, version: impl Into<String>) -> Self {
        self.version = Some(version.into());
        self
    }

    /// Sets the log level filter used when `RUST_LOG` is not set.
    ///
    /// Accepts any valid [`EnvFilter`] directive string (e.g. `"debug"`,
    /// `"facilitator=debug,r402=trace"`).
    #[must_use]
    pub fn with_log_level(mut self, level: impl Into<String>) -> Self {
        self.log_level = Some(level.into());
        self
    }

    /// Initialises the tracing subscriber and optional `OTel` exporters.
    ///
    /// Returns a [`TelemetryGuard`] that flushes exporters on drop.
    pub fn register(self) -> TelemetryGuard {
        let fallback = self.log_level.as_deref().unwrap_or("info");
        let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| fallback.into());

        #[cfg(feature = "telemetry")]
        let otel = self.init_otel();

        #[cfg(feature = "telemetry")]
        {
            let trace_layer = otel.as_ref().and_then(|g| {
                g.tracer_provider
                    .as_ref()
                    .map(|tp| OpenTelemetryLayer::new(tp.tracer("tracing-otel-subscriber")))
            });
            let metrics_layer = otel.as_ref().and_then(|g| {
                g.meter_provider
                    .as_ref()
                    .map(|mp| MetricsLayer::new(mp.clone()))
            });

            tracing_subscriber::registry()
                .with(env_filter)
                .with(tracing_subscriber::fmt::layer())
                .with(metrics_layer)
                .with(trace_layer)
                .init();
        }

        #[cfg(not(feature = "telemetry"))]
        tracing_subscriber::registry()
            .with(env_filter)
            .with(tracing_subscriber::fmt::layer())
            .init();

        #[cfg(feature = "telemetry")]
        if otel.is_some() {
            tracing::info!("OpenTelemetry exporters registered");
        }

        TelemetryGuard {
            #[cfg(feature = "telemetry")]
            _otel: otel,
        }
    }

    /// Detect OTLP configuration and initialise `OTel` providers.
    #[cfg(feature = "telemetry")]
    fn init_otel(&self) -> Option<OtelGuard> {
        let protocol = detect_protocol()?;
        let resource = self.build_resource();

        let tracer_provider = build_tracer(protocol, resource.clone());
        let meter_provider = build_meter(protocol, resource);

        Some(OtelGuard {
            tracer_provider,
            meter_provider,
        })
    }

    /// Builds an `OpenTelemetry` [`Resource`] from the resolved service identity.
    #[cfg(feature = "telemetry")]
    fn build_resource(&self) -> Resource {
        let name = resolve_otel_env("OTEL_SERVICE_NAME", self.name.as_deref());
        let version = resolve_otel_env("OTEL_SERVICE_VERSION", self.version.as_deref());
        let deployment = resolve_otel_env("OTEL_SERVICE_DEPLOYMENT", None);

        let mut builder = Resource::builder();
        if let Some(name) = name {
            builder = builder.with_service_name(name);
        }
        let mut attrs = Vec::<KeyValue>::with_capacity(2);
        if let Some(version) = version {
            attrs.push(KeyValue::new(SERVICE_VERSION, version));
        }
        if let Some(deployment) = deployment {
            attrs.push(KeyValue::new(DEPLOYMENT_ENVIRONMENT_NAME, deployment));
        }
        if !attrs.is_empty() {
            builder = builder.with_schema_url(attrs, SCHEMA_URL);
        }
        builder.build()
    }
}

/// Resolve an `OTel` env var, falling back to a programmatic default.
#[cfg(feature = "telemetry")]
fn resolve_otel_env(env_key: &str, fallback: Option<&str>) -> Option<String> {
    std::env::var(env_key)
        .ok()
        .filter(|v| !v.trim().is_empty())
        .or_else(|| fallback.map(String::from))
}

/// Detect OTLP protocol from environment. Returns `None` if `OTel` is not configured.
#[cfg(feature = "telemetry")]
fn detect_protocol() -> Option<OtlpProtocol> {
    let configured = std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT").is_ok()
        || std::env::var("OTEL_EXPORTER_OTLP_HEADERS").is_ok()
        || std::env::var("OTEL_EXPORTER_OTLP_PROTOCOL").is_ok();

    configured.then(|| {
        std::env::var("OTEL_EXPORTER_OTLP_PROTOCOL")
            .ok()
            .map_or(OtlpProtocol::Http, |s| match s.as_str() {
                "grpc" => OtlpProtocol::Grpc,
                _ => OtlpProtocol::Http,
            })
    })
}

#[cfg(feature = "telemetry")]
#[derive(Debug, Clone, Copy)]
enum OtlpProtocol {
    Http,
    Grpc,
}

#[cfg(feature = "telemetry")]
fn build_tracer(protocol: OtlpProtocol, resource: Resource) -> Option<SdkTracerProvider> {
    let exporter = match protocol {
        OtlpProtocol::Http => opentelemetry_otlp::SpanExporter::builder()
            .with_http()
            .build(),
        OtlpProtocol::Grpc => opentelemetry_otlp::SpanExporter::builder()
            .with_tonic()
            .build(),
    };

    Some(
        SdkTracerProvider::builder()
            .with_resource(resource)
            .with_batch_exporter(exporter.ok()?)
            .build(),
    )
}

#[cfg(feature = "telemetry")]
fn build_meter(protocol: OtlpProtocol, resource: Resource) -> Option<SdkMeterProvider> {
    let exporter = match protocol {
        OtlpProtocol::Http => opentelemetry_otlp::MetricExporter::builder()
            .with_http()
            .with_temporality(opentelemetry_sdk::metrics::Temporality::default())
            .build(),
        OtlpProtocol::Grpc => opentelemetry_otlp::MetricExporter::builder()
            .with_tonic()
            .with_temporality(opentelemetry_sdk::metrics::Temporality::default())
            .build(),
    };

    let reader = PeriodicReader::builder(exporter.ok()?)
        .with_interval(Duration::from_secs(30))
        .build();

    let provider = MeterProviderBuilder::default()
        .with_resource(resource)
        .with_reader(reader)
        .build();
    global::set_meter_provider(provider.clone());
    Some(provider)
}

/// Internal `OTel` state; performs graceful shutdown on drop.
#[cfg(feature = "telemetry")]
#[derive(Debug)]
struct OtelGuard {
    tracer_provider: Option<SdkTracerProvider>,
    meter_provider: Option<SdkMeterProvider>,
}

#[cfg(feature = "telemetry")]
impl Drop for OtelGuard {
    fn drop(&mut self) {
        if let Some(ref tp) = self.tracer_provider
            && let Err(err) = tp.shutdown()
        {
            tracing::error!(?err, "tracer provider shutdown error");
        }
        if let Some(ref mp) = self.meter_provider
            && let Err(err) = mp.shutdown()
        {
            tracing::error!(?err, "meter provider shutdown error");
        }
    }
}

/// Owns `OTel` providers (if active) and flushes them on drop.
///
/// Also provides [`http_trace_layer`](TelemetryGuard::http_trace_layer) for
/// Axum middleware setup.
#[derive(Debug)]
pub struct TelemetryGuard {
    #[cfg(feature = "telemetry")]
    _otel: Option<OtelGuard>,
}

impl TelemetryGuard {
    /// Creates an HTTP tracing middleware layer for Axum.
    #[must_use]
    #[allow(clippy::unused_self)]
    pub fn http_trace_layer(
        &self,
    ) -> TraceLayer<
        tower_http::classify::SharedClassifier<tower_http::classify::ServerErrorsAsFailures>,
        HttpMakeSpan,
        tower_http::trace::DefaultOnRequest,
        HttpOnResponse,
    > {
        TraceLayer::new_for_http()
            .make_span_with(HttpMakeSpan)
            .on_response(HttpOnResponse)
    }
}

/// Custom span maker for HTTP requests.
#[derive(Clone, Copy, Debug)]
pub struct HttpMakeSpan;

impl<A> MakeSpan<A> for HttpMakeSpan {
    fn make_span(&mut self, request: &Request<A>) -> Span {
        tracing::info_span!(
            "http_request",
            method = %request.method(),
            uri = %request.uri(),
            version = ?request.version(),
            status = tracing::field::Empty,
            http.status_code = tracing::field::Empty,
            otel.kind = "server",
            otel.name = %format!("{} {}", request.method(), request.uri()),
        )
    }
}

/// Custom response handler for HTTP tracing.
#[derive(Clone, Copy, Debug)]
pub struct HttpOnResponse;

impl<A> OnResponse<A> for HttpOnResponse {
    fn on_response(self, response: &Response<A>, latency: Duration, span: &Span) {
        let status = response.status();
        span.record("status", tracing::field::display(status));
        span.record("http.status_code", status.as_u16());

        #[cfg(feature = "telemetry")]
        {
            use opentelemetry::trace::Status;
            use tracing_opentelemetry::OpenTelemetrySpanExt;

            if status.is_success() {
                span.set_status(Status::Ok);
            } else {
                span.set_status(Status::error(
                    status.canonical_reason().unwrap_or("unknown"),
                ));
            }
        }

        tracing::info!(
            status = status.as_u16(),
            elapsed_ms = u64::try_from(latency.as_millis()).unwrap_or(u64::MAX),
            "request completed"
        );
    }
}
