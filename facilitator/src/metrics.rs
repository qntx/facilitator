//! Process HTTP metrics (`facilitator_http_*`).
//!
//! Uses the same `metrics` crate facade as r402-core. This binary does not
//! install a recorder; operators attach Prometheus / OTLP / `StatsD` at
//! startup. With no recorder the macros are no-ops. The `telemetry` OTLP
//! `MetricsLayer` does not scrape this facade.
//!
//! Enabling the crate `metrics` feature also turns on `r402-core/metrics`
//! so the settlement cache increments `r402_settlement_cache_reserve_total`.
//! Do **not** increment `r402_facilitator_*` from this process: those names
//! belong to r402-core and a later SDK emit would double-count.
//!
//! # `result` taxonomy
//!
//! Verify: `valid` / `invalid` / `error`. Settle: `success` / `failure` /
//! `error`.
//!
//! - `error` — HTTP 400 (`JsonRejection`), cancelled/504 timeout (unset
//!   [`VerifyMetric`] / [`SettleMetric`]), and `FacilitatorError` other than
//!   a missing handler.
//! - `invalid` / `failure` — envelope classifier, `Ok(Invalid|Failure)`, and
//!   `Aborted { reason: "no_facilitator_for_network" }` (HTTP 200 protocol
//!   miss, not infra).

use std::time::{Duration, Instant};

use r402_core::error::FacilitatorError;
use r402_core::wire::{SettleResponse, VerifyResponse};

/// `POST /verify` counter. Label `result`: `valid` | `invalid` | `error`.
pub(crate) const HTTP_VERIFY_TOTAL: &str = "facilitator_http_verify_total";

/// `POST /settle` counter. Label `result`: `success` | `failure` | `error`.
pub(crate) const HTTP_SETTLE_TOTAL: &str = "facilitator_http_settle_total";

/// `POST /verify` duration histogram, seconds.
pub(crate) const HTTP_VERIFY_DURATION_SECONDS: &str = "facilitator_http_verify_duration_seconds";

/// `POST /settle` duration histogram, seconds.
pub(crate) const HTTP_SETTLE_DURATION_SECONDS: &str = "facilitator_http_settle_duration_seconds";

/// Closed `result` set for `/verify`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum VerifyResult {
    /// `VerifyResponse::Valid`.
    Valid,
    /// Envelope reject, `Ok(Invalid)`, or missing handler.
    Invalid,
    /// 400, timeout/cancel, or transport/internal `FacilitatorError`.
    Error,
}

impl VerifyResult {
    /// Prometheus `result` label.
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Valid => "valid",
            Self::Invalid => "invalid",
            Self::Error => "error",
        }
    }

    /// Maps a completed verify body.
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
    /// Envelope reject, `Ok(Failure)`, or missing handler.
    Failure,
    /// 400, timeout/cancel, or transport/internal `FacilitatorError`.
    Error,
}

impl SettleResult {
    /// Prometheus `result` label.
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Success => "success",
            Self::Failure => "failure",
            Self::Error => "error",
        }
    }

    /// Maps a completed settle body.
    pub(crate) const fn from_response(response: &SettleResponse) -> Self {
        if response.is_success() {
            Self::Success
        } else {
            Self::Failure
        }
    }
}

/// Registry abort is a protocol 200, not an infra `error` page.
fn is_missing_handler(error: &FacilitatorError) -> bool {
    matches!(
        error,
        FacilitatorError::Aborted { reason, .. } if reason == "no_facilitator_for_network"
    )
}

/// `FacilitatorError` → verify `result`. Missing handler is `invalid`.
pub(crate) fn verify_from_error(error: &FacilitatorError) -> VerifyResult {
    if is_missing_handler(error) {
        VerifyResult::Invalid
    } else {
        VerifyResult::Error
    }
}

/// `FacilitatorError` → settle `result`. Missing handler is `failure`.
pub(crate) fn settle_from_error(error: &FacilitatorError) -> SettleResult {
    if is_missing_handler(error) {
        SettleResult::Failure
    } else {
        SettleResult::Error
    }
}

/// Records `/verify` on drop. Unset result (timeout cancel) is `error`.
#[derive(Debug)]
#[must_use = "records on drop; timeout cancel is result=error"]
pub(crate) struct VerifyMetric {
    /// Handler start; elapsed is recorded even when the future is cancelled.
    started: Instant,
    /// `None` until `finish`; `None` at drop means 504/cancel.
    result: Option<VerifyResult>,
}

impl VerifyMetric {
    /// Starts the verify timer.
    pub(crate) fn start() -> Self {
        Self {
            started: Instant::now(),
            result: None,
        }
    }

    /// Sets the label recorded on drop.
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
    /// Handler start; elapsed is recorded even when the future is cancelled.
    started: Instant,
    /// `None` until `finish`; `None` at drop means 504/cancel.
    result: Option<SettleResult>,
}

impl SettleMetric {
    /// Starts the settle timer.
    pub(crate) fn start() -> Self {
        Self {
            started: Instant::now(),
            result: None,
        }
    }

    /// Sets the label recorded on drop.
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

/// Records one `/verify` outcome.
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
        ::metrics::counter!(HTTP_VERIFY_TOTAL, "result" => result).increment(1);
        ::metrics::histogram!(HTTP_VERIFY_DURATION_SECONDS, "result" => result)
            .record(duration.as_secs_f64());
    }
    #[cfg(not(feature = "metrics"))]
    let _ = (
        HTTP_VERIFY_TOTAL,
        HTTP_VERIFY_DURATION_SECONDS,
        result,
        duration,
    );
}

/// Records one `/settle` outcome.
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
        ::metrics::counter!(HTTP_SETTLE_TOTAL, "result" => result).increment(1);
        ::metrics::histogram!(HTTP_SETTLE_DURATION_SECONDS, "result" => result)
            .record(duration.as_secs_f64());
    }
    #[cfg(not(feature = "metrics"))]
    let _ = (
        HTTP_SETTLE_TOTAL,
        HTTP_SETTLE_DURATION_SECONDS,
        result,
        duration,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_process_http_not_r402_facilitator() {
        for name in [
            HTTP_VERIFY_TOTAL,
            HTTP_SETTLE_TOTAL,
            HTTP_VERIFY_DURATION_SECONDS,
            HTTP_SETTLE_DURATION_SECONDS,
        ] {
            assert!(
                name.starts_with("facilitator_http_"),
                "{name} must be process-prefixed"
            );
            assert!(
                !name.starts_with("r402_facilitator_"),
                "{name} must not use the SDK prefix"
            );
        }
    }

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
    fn missing_handler_is_protocol_not_infra() {
        let err = FacilitatorError::aborted(
            "no_facilitator_for_network",
            "no handler registered for this payment scheme",
        );
        assert_eq!(
            verify_from_error(&err),
            VerifyResult::Invalid,
            "verify missing handler"
        );
        assert_eq!(
            settle_from_error(&err),
            SettleResult::Failure,
            "settle missing handler"
        );
    }

    #[test]
    fn onchain_error_is_infra() {
        let err = FacilitatorError::Onchain("rpc down".into());
        assert_eq!(
            verify_from_error(&err),
            VerifyResult::Error,
            "verify onchain"
        );
        assert_eq!(
            settle_from_error(&err),
            SettleResult::Error,
            "settle onchain"
        );
    }

    #[test]
    fn record_without_recorder_does_not_panic() {
        record_verify(VerifyResult::Valid, Duration::from_millis(1));
        record_settle(SettleResult::Success, Duration::from_millis(1));
    }

    #[cfg(feature = "metrics")]
    #[test]
    fn drop_without_finish_records_error() {
        let recorder = metrics_util::debugging::DebuggingRecorder::new();
        let snapshotter = recorder.snapshotter();
        metrics::with_local_recorder(&recorder, || {
            drop(VerifyMetric::start());
            drop(SettleMetric::start());
        });
        let rows = snapshotter.snapshot().into_vec();
        assert!(
            rows.iter().any(|(ck, _, _, value)| {
                ck.key().name() == HTTP_VERIFY_TOTAL
                    && ck
                        .key()
                        .labels()
                        .any(|l| l.key() == "result" && l.value() == "error")
                    && matches!(value, metrics_util::debugging::DebugValue::Counter(1))
            }),
            "verify cancel → error: {rows:?}"
        );
        assert!(
            rows.iter().any(|(ck, _, _, value)| {
                ck.key().name() == HTTP_SETTLE_TOTAL
                    && ck
                        .key()
                        .labels()
                        .any(|l| l.key() == "result" && l.value() == "error")
                    && matches!(value, metrics_util::debugging::DebugValue::Counter(1))
            }),
            "settle cancel → error: {rows:?}"
        );
        assert!(
            rows.iter()
                .all(|(ck, _, _, _)| !ck.key().name().starts_with("r402_facilitator_")),
            "must not emit r402_facilitator_*: {rows:?}"
        );
    }
}
