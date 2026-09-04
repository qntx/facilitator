//! Process HTTP metrics (`r402_facilitator_*`).
//!
//! Names come from [`r402_protocol::metrics`]. This process installs the
//! Prometheus recorder when `http.metrics_listen` is set. `/metrics` is bound
//! only on that listen, never on the protocol port.
//!
//! # `result` taxonomy
//!
//! Verify: `valid` / `invalid` / `error`. Settle: `success` / `failure` /
//! `error`.
//!
//! - `error` — HTTP 400, 502, 504/cancel.
//! - `invalid` / `failure` — envelope classifier, `Ok(Invalid|Failure)`, and
//!   `Err` converted to protocol JSON (including Onchain).

use std::time::{Duration, Instant};

use axum::Router;
use axum::extract::State;
use axum::http::{StatusCode, header};
use axum::response::IntoResponse;
use axum::routing::get;
use r402_protocol::error::FacilitatorError;
use r402_protocol::payment::{SettleResponse, VerifyResponse};
use tower_http::timeout::TimeoutLayer;

use crate::error::Error;

/// Prometheus scrape timeout.
const METRICS_TIMEOUT: Duration = Duration::from_secs(5);

/// Handle used to render Prometheus text. `None` skips global recorder install.
#[derive(Clone)]
pub struct MetricsHandle {
    #[cfg(feature = "metrics")]
    inner: Option<metrics_exporter_prometheus::PrometheusHandle>,
    #[cfg(not(feature = "metrics"))]
    _private: (),
}

impl std::fmt::Debug for MetricsHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MetricsHandle").finish_non_exhaustive()
    }
}

impl MetricsHandle {
    /// No recorder. `GET /metrics` returns an empty body.
    #[must_use]
    pub const fn disabled() -> Self {
        Self {
            #[cfg(feature = "metrics")]
            inner: None,
            #[cfg(not(feature = "metrics"))]
            _private: (),
        }
    }

    fn render(&self) -> String {
        #[cfg(feature = "metrics")]
        {
            self.inner.as_ref().map_or_else(
                String::new,
                metrics_exporter_prometheus::PrometheusHandle::render,
            )
        }
        #[cfg(not(feature = "metrics"))]
        {
            let () = self._private;
            String::new()
        }
    }
}

/// Install the global Prometheus recorder.
///
/// # Errors
///
/// Recorder already installed, or the `metrics` feature is off.
pub(crate) fn install() -> Result<MetricsHandle, Error> {
    #[cfg(feature = "metrics")]
    {
        let inner = metrics_exporter_prometheus::PrometheusBuilder::new()
            .install_recorder()
            .map_err(|err| Error::server_with("failed to install prometheus recorder", err))?;
        describe_metrics();
        Ok(MetricsHandle { inner: Some(inner) })
    }
    #[cfg(not(feature = "metrics"))]
    Err(Error::server(
        "http.metrics_listen is set but the metrics feature is disabled",
    ))
}

/// Metrics listen router. Not merged into the protocol port.
pub fn metrics_router(handle: MetricsHandle) -> Router {
    Router::new()
        .route("/metrics", get(scrape))
        .layer(TimeoutLayer::with_status_code(
            StatusCode::GATEWAY_TIMEOUT,
            METRICS_TIMEOUT,
        ))
        .with_state(handle)
}

async fn scrape(State(handle): State<MetricsHandle>) -> impl IntoResponse {
    (
        [(
            header::CONTENT_TYPE,
            "text/plain; version=0.0.4; charset=utf-8",
        )],
        handle.render(),
    )
}

#[cfg(feature = "metrics")]
fn describe_metrics() {
    metrics::describe_counter!(
        r402_protocol::metrics::FACILITATOR_VERIFY_TOTAL,
        "Facilitator verify outcomes"
    );
    metrics::describe_counter!(
        r402_protocol::metrics::FACILITATOR_SETTLE_TOTAL,
        "Facilitator settle outcomes"
    );
    metrics::describe_histogram!(
        r402_protocol::metrics::FACILITATOR_VERIFY_DURATION_SECONDS,
        "Facilitator verify duration"
    );
    metrics::describe_histogram!(
        r402_protocol::metrics::FACILITATOR_SETTLE_DURATION_SECONDS,
        "Facilitator settle duration"
    );
}

/// Closed `result` set for `/verify`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum VerifyResult {
    /// `VerifyResponse::Valid`.
    Valid,
    /// Envelope reject, `Ok(Invalid)`, or protocol `Err`.
    Invalid,
    /// 400, 502, timeout/cancel.
    Error,
}

impl VerifyResult {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Valid => "valid",
            Self::Invalid => "invalid",
            Self::Error => "error",
        }
    }

    pub(crate) const fn from_response(response: &VerifyResponse) -> Self {
        if response.is_valid() {
            Self::Valid
        } else {
            Self::Invalid
        }
    }
}

/// Closed `result` set for `/settle`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SettleResult {
    /// `SettleResponse::Success`.
    Success,
    /// Envelope reject, `Ok(Failure)`, or protocol `Err`.
    Failure,
    /// 400, 502, timeout/cancel.
    Error,
}

impl SettleResult {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Success => "success",
            Self::Failure => "failure",
            Self::Error => "error",
        }
    }

    pub(crate) const fn from_response(response: &SettleResponse) -> Self {
        if response.is_success() {
            Self::Success
        } else {
            Self::Failure
        }
    }
}

/// `FacilitatorError` → verify `result`. Transport is `error`; protocol JSON is `invalid`.
pub(crate) fn verify_from_error(error: &FacilitatorError) -> VerifyResult {
    if VerifyResponse::from_facilitator_error(error).is_some() {
        VerifyResult::Invalid
    } else {
        VerifyResult::Error
    }
}

/// `FacilitatorError` → settle `result`. Transport is `error`; protocol JSON is `failure`.
pub(crate) fn settle_from_error(error: &FacilitatorError) -> SettleResult {
    if SettleResponse::from_facilitator_error(error, "", "").is_some() {
        SettleResult::Failure
    } else {
        SettleResult::Error
    }
}

/// Records `/verify` on drop. Unset result (timeout cancel) is `error`.
#[derive(Debug)]
#[must_use = "records on drop; timeout cancel is result=error"]
pub(crate) struct VerifyMetric {
    started: Instant,
    result: Option<VerifyResult>,
}

impl VerifyMetric {
    pub(crate) fn start() -> Self {
        Self {
            started: Instant::now(),
            result: None,
        }
    }

    pub(crate) const fn finish(&mut self, result: VerifyResult) {
        self.result = Some(result);
    }
}

impl Drop for VerifyMetric {
    fn drop(&mut self) {
        record_verify(
            self.result.unwrap_or(VerifyResult::Error),
            self.started.elapsed(),
        );
    }
}

/// Records `/settle` on drop. Unset result (timeout cancel) is `error`.
#[derive(Debug)]
#[must_use = "records on drop; timeout cancel is result=error"]
pub(crate) struct SettleMetric {
    started: Instant,
    result: Option<SettleResult>,
}

impl SettleMetric {
    pub(crate) fn start() -> Self {
        Self {
            started: Instant::now(),
            result: None,
        }
    }

    pub(crate) const fn finish(&mut self, result: SettleResult) {
        self.result = Some(result);
    }
}

impl Drop for SettleMetric {
    fn drop(&mut self) {
        record_settle(
            self.result.unwrap_or(SettleResult::Error),
            self.started.elapsed(),
        );
    }
}

#[cfg_attr(
    not(feature = "metrics"),
    allow(
        clippy::missing_const_for_fn,
        reason = "metrics-enabled body calls the metrics facade"
    )
)]
pub(crate) fn record_verify(result: VerifyResult, duration: Duration) {
    let result = result.as_str();
    #[cfg(feature = "metrics")]
    {
        metrics::counter!(
            r402_protocol::metrics::FACILITATOR_VERIFY_TOTAL,
            "result" => result
        )
        .increment(1);
        metrics::histogram!(
            r402_protocol::metrics::FACILITATOR_VERIFY_DURATION_SECONDS,
            "result" => result
        )
        .record(duration.as_secs_f64());
    }
    #[cfg(not(feature = "metrics"))]
    let _ = (result, duration);
}

#[cfg_attr(
    not(feature = "metrics"),
    allow(
        clippy::missing_const_for_fn,
        reason = "metrics-enabled body calls the metrics facade"
    )
)]
pub(crate) fn record_settle(result: SettleResult, duration: Duration) {
    let result = result.as_str();
    #[cfg(feature = "metrics")]
    {
        metrics::counter!(
            r402_protocol::metrics::FACILITATOR_SETTLE_TOTAL,
            "result" => result
        )
        .increment(1);
        metrics::histogram!(
            r402_protocol::metrics::FACILITATOR_SETTLE_DURATION_SECONDS,
            "result" => result
        )
        .record(duration.as_secs_f64());
    }
    #[cfg(not(feature = "metrics"))]
    let _ = (result, duration);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn result_labels_are_closed() {
        assert_eq!(VerifyResult::Valid.as_str(), "valid", "valid");
        assert_eq!(VerifyResult::Invalid.as_str(), "invalid", "invalid");
        assert_eq!(VerifyResult::Error.as_str(), "error", "error");
        assert_eq!(SettleResult::Success.as_str(), "success", "success");
        assert_eq!(SettleResult::Failure.as_str(), "failure", "failure");
        assert_eq!(SettleResult::Error.as_str(), "error", "error");
    }

    #[test]
    fn transport_is_infra_error() {
        let err = FacilitatorError::transport(r402_protocol::error::FacilitatorTransportKind::Io);
        assert_eq!(
            verify_from_error(&err),
            VerifyResult::Error,
            "verify transport"
        );
        assert_eq!(
            settle_from_error(&err),
            SettleResult::Error,
            "settle transport"
        );
    }

    #[test]
    fn onchain_is_protocol_not_infra() {
        let err = FacilitatorError::Onchain("rpc down".into());
        assert_eq!(
            verify_from_error(&err),
            VerifyResult::Invalid,
            "verify onchain"
        );
        assert_eq!(
            settle_from_error(&err),
            SettleResult::Failure,
            "settle onchain"
        );
    }

    #[test]
    fn record_without_recorder_does_not_panic() {
        record_verify(VerifyResult::Valid, Duration::from_millis(1));
        record_settle(SettleResult::Success, Duration::from_millis(1));
    }
}
