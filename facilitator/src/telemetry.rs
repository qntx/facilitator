//! `OpenTelemetry` tracing and metrics setup.
//!
//! Provides [`Telemetry`] for configuring distributed tracing and metrics
//! collection via OTLP exporters. Only available with the `telemetry` feature.

use std::env;
use std::time::Duration;

use axum::http::{Request, Response};
use opentelemetry::trace::{Status, TracerProvider};
use opentelemetry::{KeyValue, Value, global};
use opentelemetry_sdk::{
    Resource,
    metrics::{MeterProviderBuilder, PeriodicReader, SdkMeterProvider},
    trace::{RandomIdGenerator, Sampler, SdkTracerProvider},
};
use opentelemetry_semantic_conventions::{
    SCHEMA_URL,
    attribute::{DEPLOYMENT_ENVIRONMENT_NAME, SERVICE_VERSION},
};
use tower_http::trace::{MakeSpan, OnResponse, TraceLayer};
use tracing::Span;
use tracing_opentelemetry::{MetricsLayer, OpenTelemetryLayer, OpenTelemetrySpanExt};
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

/// Resolve an env var with a programmatic fallback.
fn resolve_env(env_key: &str, fallback: Option<&Value>) -> Option<Value> {
    env::var(env_key)
        .ok()
        .filter(|v| !v.trim().is_empty())
        .map(Value::from)
        .or_else(|| fallback.cloned())
}

/// Detects OTLP protocol from environment. Returns `None` if OTEL is not configured.
fn detect_protocol() -> Option<OtlpProtocol> {
    let is_enabled = env::var("OTEL_EXPORTER_OTLP_ENDPOINT").is_ok()
        || env::var("OTEL_EXPORTER_OTLP_HEADERS").is_ok()
        || env::var("OTEL_EXPORTER_OTLP_PROTOCOL").is_ok();
    is_enabled.then(|| {
        env::var("OTEL_EXPORTER_OTLP_PROTOCOL")
            .ok()
            .map_or(OtlpProtocol::Http, |s| match s.as_str() {
                "grpc" => OtlpProtocol::Grpc,
                _ => OtlpProtocol::Http,
            })
    })
}

/// Supported OTLP transport protocols.
#[derive(Debug, Clone, Copy)]
enum OtlpProtocol {
    Http,
    Grpc,
}

/// Service identity for telemetry resources.
///
/// Values can be set programmatically or overridden via `OTEL_SERVICE_NAME`,
/// `OTEL_SERVICE_VERSION`, `OTEL_SERVICE_DEPLOYMENT` environment variables.
#[derive(Debug, Default)]
pub struct Telemetry {
    name: Option<Value>,
    version: Option<Value>,
    deployment: Option<Value>,
    log_level: Option<String>,
}

impl Telemetry {
    /// Creates a new, empty [`Telemetry`] instance.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the service name.
    #[must_use]
    pub fn with_name(mut self, name: impl Into<Value>) -> Self {
        self.name = Some(name.into());
        self
    }

    /// Sets the service version.
    #[must_use]
    pub fn with_version(mut self, version: impl Into<Value>) -> Self {
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

    /// Builds an `OpenTelemetry` [`Resource`] from the resolved service identity.
    fn resource(&self) -> Resource {
        let name = resolve_env("OTEL_SERVICE_NAME", self.name.as_ref());
        let version = resolve_env("OTEL_SERVICE_VERSION", self.version.as_ref());
        let deployment = resolve_env("OTEL_SERVICE_DEPLOYMENT", self.deployment.as_ref());

        let mut builder = Resource::builder();
        if let Some(name) = name {
            builder = builder.with_service_name(name);
        }
        let mut attributes = Vec::<KeyValue>::with_capacity(2);
        if let Some(version) = version {
            attributes.push(KeyValue::new(SERVICE_VERSION, version));
        }
        if let Some(deployment) = deployment {
            attributes.push(KeyValue::new(DEPLOYMENT_ENVIRONMENT_NAME, deployment));
        }
        if !attributes.is_empty() {
            builder = builder.with_schema_url(attributes, SCHEMA_URL);
        }
        builder.build()
    }

    /// Initializes the tracer provider.
    fn init_tracer(&self, protocol: OtlpProtocol) -> Option<SdkTracerProvider> {
        let exporter = match protocol {
            OtlpProtocol::Http => opentelemetry_otlp::SpanExporter::builder()
                .with_http()
                .build(),
            OtlpProtocol::Grpc => opentelemetry_otlp::SpanExporter::builder()
                .with_tonic()
                .build(),
        };
        let exporter = exporter.ok()?;

        Some(
            SdkTracerProvider::builder()
                .with_sampler(Sampler::ParentBased(Box::new(Sampler::TraceIdRatioBased(
                    1.0,
                ))))
                .with_id_generator(RandomIdGenerator::default())
                .with_resource(self.resource())
                .with_batch_exporter(exporter)
                .build(),
        )
    }

    /// Initializes the metrics provider.
    fn init_meter(&self, protocol: OtlpProtocol) -> Option<SdkMeterProvider> {
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
        let exporter = exporter.ok()?;

        let reader = PeriodicReader::builder(exporter)
            .with_interval(Duration::from_secs(30))
            .build();
        let stdout_reader =
            PeriodicReader::builder(opentelemetry_stdout::MetricExporter::default()).build();

        let provider = MeterProviderBuilder::default()
            .with_resource(self.resource())
            .with_reader(reader)
            .with_reader(stdout_reader)
            .build();
        global::set_meter_provider(provider.clone());
        Some(provider)
    }

    /// Registers tracing and metrics exporters.
    ///
    /// When `OTEL_EXPORTER_OTLP_*` env vars are present, enables OTLP export.
    /// Otherwise falls back to console logging.
    ///
    /// Returns [`TelemetryGuard`] that flushes exporters on drop.
    pub fn register(self) -> TelemetryGuard {
        let protocol = detect_protocol();
        let (tracer_provider, meter_provider) = protocol.map_or_else(
            || (None, None),
            |p| (self.init_tracer(p), self.init_meter(p)),
        );

        // Build subscriber: Option<Layer> is a no-op when None
        let otel_layer = tracer_provider
            .as_ref()
            .map(|tp| OpenTelemetryLayer::new(tp.tracer("tracing-otel-subscriber")));
        let metrics_layer = meter_provider
            .as_ref()
            .map(|mp| MetricsLayer::new(mp.clone()));

        let fallback = self.log_level.as_deref().unwrap_or("info");
        tracing_subscriber::registry()
            .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| fallback.into()))
            .with(tracing_subscriber::fmt::layer())
            .with(metrics_layer)
            .with(otel_layer)
            .init();

        if protocol.is_some() {
            tracing::info!("OpenTelemetry exporters registered");
        } else {
            tracing::info!("OpenTelemetry is not configured, console logging only");
        }

        TelemetryGuard {
            tracer_provider,
            meter_provider,
        }
    }
}

/// Owns the tracer and meter providers; performs graceful shutdown on drop.
#[derive(Debug)]
pub struct TelemetryGuard {
    tracer_provider: Option<SdkTracerProvider>,
    meter_provider: Option<SdkMeterProvider>,
}

impl Drop for TelemetryGuard {
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

impl TelemetryGuard {
    /// Creates an HTTP tracing layer for axum applications.
    #[must_use]
    #[allow(clippy::unused_self)]
    pub fn http_tracing(
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
            otel.kind = "server",
            otel.name = %format!("{} {}", request.method(), request.uri()),
            method = %request.method(),
            uri = %request.uri(),
            version = ?request.version(),
        )
    }
}

/// Custom response handler for HTTP tracing.
#[derive(Clone, Copy, Debug)]
pub struct HttpOnResponse;

impl<A> OnResponse<A> for HttpOnResponse {
    fn on_response(self, response: &Response<A>, latency: Duration, span: &Span) {
        span.record("status", tracing::field::display(response.status()));
        span.record(
            "http.status_code",
            tracing::field::display(response.status().as_u16()),
        );

        if response.status().is_success() {
            span.set_status(Status::Ok);
        } else {
            span.set_status(Status::error(
                response
                    .status()
                    .canonical_reason()
                    .unwrap_or("unknown")
                    .to_string(),
            ));
        }

        tracing::info!(
            "status={} elapsed={}ms",
            response.status().as_u16(),
            latency.as_millis()
        );
    }
}
