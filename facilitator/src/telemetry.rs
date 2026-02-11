//! `OpenTelemetry` tracing and metrics setup.
//!
//! Provides [`Telemetry`] for configuring distributed tracing and metrics
//! collection via OTLP exporters. Only available with the `telemetry` feature.
//!
//! # Environment Variables
//!
//! | Variable | Description |
//! |----------|-------------|
//! | `OTEL_EXPORTER_OTLP_ENDPOINT` | OTLP collector endpoint |
//! | `OTEL_EXPORTER_OTLP_PROTOCOL` | Protocol (`http/protobuf` or `grpc`) |
//! | `OTEL_SERVICE_NAME` | Service name for traces |
//! | `OTEL_SERVICE_VERSION` | Service version |
//! | `OTEL_SERVICE_DEPLOYMENT` | Deployment environment |

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
use serde::{Deserialize, Serialize};
use tower_http::trace::{MakeSpan, OnResponse, TraceLayer};
use tracing::Span;
use tracing_opentelemetry::{MetricsLayer, OpenTelemetryLayer, OpenTelemetrySpanExt};
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

/// Supported OTLP transport protocols.
#[allow(clippy::upper_case_acronyms)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
enum TelemetryProtocol {
    /// `http/protobuf` protocol.
    #[serde(rename = "http/protobuf")]
    HTTP,
    /// `grpc` protocol.
    #[serde(rename = "grpc")]
    GRPC,
}

impl TelemetryProtocol {
    /// Detects protocol from `OTEL_*` environment variables.
    /// Returns `None` if telemetry is not enabled.
    pub fn from_env() -> Option<Self> {
        let is_enabled = env::var("OTEL_EXPORTER_OTLP_ENDPOINT").is_ok()
            || env::var("OTEL_EXPORTER_OTLP_HEADERS").is_ok()
            || env::var("OTEL_EXPORTER_OTLP_PROTOCOL").is_ok();
        is_enabled.then(|| {
            env::var("OTEL_EXPORTER_OTLP_PROTOCOL")
                .ok()
                .map_or(Self::HTTP, |s| match s.as_str() {
                    "grpc" => Self::GRPC,
                    _ => Self::HTTP,
                })
        })
    }
}

/// Service identity and metadata for telemetry resources.
///
/// Values can be set programmatically or overridden via environment variables
/// (`OTEL_SERVICE_NAME`, `OTEL_SERVICE_VERSION`, `OTEL_SERVICE_DEPLOYMENT`).
#[derive(Clone, Debug, Default)]
pub struct Telemetry {
    /// Optional service name.
    pub name: Option<Value>,
    /// Optional service version.
    pub version: Option<Value>,
    /// Optional deployment environment.
    pub deployment: Option<Value>,
}

impl Telemetry {
    /// Creates a new, empty [`Telemetry`] instance.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the service name.
    #[must_use]
    #[allow(dead_code)]
    pub fn with_name(&self, name: impl Into<Value>) -> Self {
        let mut this = self.clone();
        this.name = Some(name.into());
        this
    }

    /// Sets the service version.
    #[must_use]
    #[allow(dead_code)]
    pub fn with_version(&self, version: impl Into<Value>) -> Self {
        let mut this = self.clone();
        this.version = Some(version.into());
        this
    }

    /// Sets the deployment environment (e.g. `"production"`, `"staging"`).
    #[must_use]
    #[allow(dead_code)]
    pub fn with_deployment(&self, deployment: impl Into<Value>) -> Self {
        let mut this = self.clone();
        this.deployment = Some(deployment.into());
        this
    }

    /// Resolves the service name (`OTEL_SERVICE_NAME` env → programmatic value).
    pub fn name(&self) -> Option<Value> {
        env::var("OTEL_SERVICE_NAME")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .map(Value::from)
            .or_else(|| self.name.clone())
    }

    /// Resolves the service version (`OTEL_SERVICE_VERSION` env → programmatic value).
    pub fn version(&self) -> Option<Value> {
        env::var("OTEL_SERVICE_VERSION")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .map(Value::from)
            .or_else(|| self.version.clone())
    }

    /// Resolves the deployment environment (`OTEL_SERVICE_DEPLOYMENT` env → programmatic value).
    pub fn deployment(&self) -> Option<Value> {
        env::var("OTEL_SERVICE_DEPLOYMENT")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .map(Value::from)
            .or_else(|| self.deployment.clone())
    }

    /// Builds an `OpenTelemetry` [`Resource`] from the resolved service identity.
    #[must_use]
    pub fn resource(&self) -> Resource {
        let mut builder = Resource::builder();
        if let Some(name) = self.name() {
            builder = builder.with_service_name(name);
        }
        let mut attributes = Vec::<KeyValue>::with_capacity(2);
        if let Some(version) = self.version() {
            attributes.push(KeyValue::new(SERVICE_VERSION, version));
        }
        if let Some(deployment) = self.deployment() {
            attributes.push(KeyValue::new(DEPLOYMENT_ENVIRONMENT_NAME, deployment));
        }
        if !attributes.is_empty() {
            builder = builder.with_schema_url(attributes, SCHEMA_URL);
        }
        builder.build()
    }

    /// Initializes the tracer provider.
    ///
    /// Returns `None` if the OTLP exporter cannot be built (graceful degradation).
    fn init_tracer_provider(&self, protocol: TelemetryProtocol) -> Option<SdkTracerProvider> {
        let exporter = opentelemetry_otlp::SpanExporter::builder();
        let exporter = match protocol {
            TelemetryProtocol::HTTP => exporter.with_http().build(),
            TelemetryProtocol::GRPC => exporter.with_tonic().build(),
        };
        let exporter = match exporter {
            Ok(e) => e,
            Err(err) => {
                eprintln!("Failed to build OTLP span exporter: {err}, falling back to console");
                return None;
            }
        };

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
    ///
    /// Returns `None` if the OTLP exporter cannot be built (graceful degradation).
    fn init_meter_provider(&self, protocol: TelemetryProtocol) -> Option<SdkMeterProvider> {
        let exporter = opentelemetry_otlp::MetricExporter::builder();
        let exporter = match protocol {
            TelemetryProtocol::HTTP => exporter
                .with_http()
                .with_temporality(opentelemetry_sdk::metrics::Temporality::default())
                .build(),
            TelemetryProtocol::GRPC => exporter
                .with_tonic()
                .with_temporality(opentelemetry_sdk::metrics::Temporality::default())
                .build(),
        };
        let exporter = match exporter {
            Ok(e) => e,
            Err(err) => {
                eprintln!("Failed to build OTLP metric exporter: {err}, falling back to console");
                return None;
            }
        };

        let reader = PeriodicReader::builder(exporter)
            .with_interval(Duration::from_secs(30))
            .build();

        let stdout_reader =
            PeriodicReader::builder(opentelemetry_stdout::MetricExporter::default()).build();

        let meter_provider = MeterProviderBuilder::default()
            .with_resource(self.resource())
            .with_reader(reader)
            .with_reader(stdout_reader)
            .build();

        global::set_meter_provider(meter_provider.clone());
        Some(meter_provider)
    }

    /// Registers tracing and metrics exporters.
    ///
    /// When `OTEL_EXPORTER_OTLP_*` env vars are present, enables OTLP export.
    /// Otherwise falls back to console logging.
    ///
    /// Returns [`TelemetryProviders`] that flushes exporters on drop.
    #[allow(clippy::option_if_let_else)]
    pub fn register(&self) -> TelemetryProviders {
        let telemetry_protocol = TelemetryProtocol::from_env();
        if let Some(protocol) = telemetry_protocol {
            let tracer_provider = self.init_tracer_provider(protocol);
            let meter_provider = self.init_meter_provider(protocol);

            // Graceful degradation: if either provider fails, fall back to console-only
            if let Some(ref tp) = tracer_provider {
                let tracer = tp.tracer("tracing-otel-subscriber");
                if let Some(ref mp) = meter_provider {
                    tracing_subscriber::registry()
                        .with(tracing_subscriber::filter::LevelFilter::INFO)
                        .with(tracing_subscriber::fmt::layer())
                        .with(MetricsLayer::new(mp.clone()))
                        .with(OpenTelemetryLayer::new(tracer))
                        .init();
                } else {
                    tracing_subscriber::registry()
                        .with(tracing_subscriber::filter::LevelFilter::INFO)
                        .with(tracing_subscriber::fmt::layer())
                        .with(OpenTelemetryLayer::new(tracer))
                        .init();
                }
                tracing::info!(
                    "OpenTelemetry tracing exporter is enabled via {:?}",
                    protocol
                );
            } else {
                tracing_subscriber::registry()
                    .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
                    .with(tracing_subscriber::fmt::layer())
                    .init();
                tracing::warn!("OpenTelemetry exporters failed to initialize, using console only");
            }

            TelemetryProviders {
                tracer_provider,
                meter_provider,
            }
        } else {
            tracing_subscriber::registry()
                .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
                .with(tracing_subscriber::fmt::layer())
                .init();

            tracing::info!("OpenTelemetry is not enabled");

            TelemetryProviders {
                tracer_provider: None,
                meter_provider: None,
            }
        }
    }
}

/// Owns the tracer and meter providers; performs graceful shutdown on drop.
#[derive(Debug)]
pub struct TelemetryProviders {
    /// Tracer provider for `OpenTelemetry` spans.
    pub tracer_provider: Option<SdkTracerProvider>,
    /// Metrics provider for `OpenTelemetry` metrics.
    pub meter_provider: Option<SdkMeterProvider>,
}

impl Drop for TelemetryProviders {
    fn drop(&mut self) {
        if let Some(tracer_provider) = self.tracer_provider.as_ref()
            && let Err(err) = tracer_provider.shutdown()
        {
            tracing::error!(?err, "tracer provider shutdown error");
        }
        if let Some(meter_provider) = self.meter_provider.as_ref()
            && let Err(err) = meter_provider.shutdown()
        {
            tracing::error!(?err, "meter provider shutdown error");
        }
    }
}

impl TelemetryProviders {
    /// Creates an HTTP tracing layer for axum applications.
    #[must_use]
    #[allow(clippy::unused_self)]
    pub fn http_tracing(
        &self,
    ) -> TraceLayer<
        tower_http::classify::SharedClassifier<tower_http::classify::ServerErrorsAsFailures>,
        FacilitatorHttpMakeSpan,
        tower_http::trace::DefaultOnRequest,
        FacilitatorHttpOnResponse,
    > {
        TraceLayer::new_for_http()
            .make_span_with(FacilitatorHttpMakeSpan)
            .on_response(FacilitatorHttpOnResponse)
    }
}

/// Custom span maker for HTTP requests.
#[derive(Clone, Copy, Debug)]
pub struct FacilitatorHttpMakeSpan;

impl<A> MakeSpan<A> for FacilitatorHttpMakeSpan {
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
pub struct FacilitatorHttpOnResponse;

impl<A> OnResponse<A> for FacilitatorHttpOnResponse {
    fn on_response(self, response: &Response<A>, latency: Duration, span: &Span) {
        span.record("status", tracing::field::display(response.status()));
        span.record("latency", tracing::field::display(latency.as_millis()));
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
