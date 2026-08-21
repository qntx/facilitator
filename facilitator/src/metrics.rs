//! Process HTTP metrics (`facilitator_http_*`).
//!
//! Uses the same `metrics` crate facade as r402-core. This binary does not
//! install a recorder; operators attach Prometheus / OTLP / `StatsD` at
//! startup. With no recorder the macros are no-ops.
//!
//! Enabling the crate `metrics` feature also turns on `r402-core/metrics`
//! so the settlement cache increments `r402_settlement_cache_reserve_total`.
//! Do **not** increment `r402_facilitator_*` from this process: those names
//! belong to r402-core and a later SDK emit would double-count.

use std::time::Duration;

/// `POST /verify` counter. Label `result`: `valid` | `invalid` | `error`.
pub(crate) const HTTP_VERIFY_TOTAL: &str = "facilitator_http_verify_total";

/// `POST /settle` counter. Label `result`: `success` | `failure` | `error`.
pub(crate) const HTTP_SETTLE_TOTAL: &str = "facilitator_http_settle_total";

/// `POST /verify` duration histogram, seconds.
pub(crate) const HTTP_VERIFY_DURATION_SECONDS: &str = "facilitator_http_verify_duration_seconds";

/// `POST /settle` duration histogram, seconds.
pub(crate) const HTTP_SETTLE_DURATION_SECONDS: &str = "facilitator_http_settle_duration_seconds";

/// Records one `/verify` outcome.
///
/// `result` is `valid`, `invalid`, or `error`.
#[cfg_attr(
    not(feature = "metrics"),
    allow(
        clippy::missing_const_for_fn,
        reason = "metrics-enabled body calls the metrics facade"
    )
)]
pub(crate) fn record_verify(result: &'static str, duration: Duration) {
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
///
/// `result` is `success`, `failure`, or `error`.
#[cfg_attr(
    not(feature = "metrics"),
    allow(
        clippy::missing_const_for_fn,
        reason = "metrics-enabled body calls the metrics facade"
    )
)]
pub(crate) fn record_settle(result: &'static str, duration: Duration) {
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
    fn record_without_recorder_does_not_panic() {
        record_verify("valid", Duration::from_millis(1));
        record_verify("invalid", Duration::from_millis(1));
        record_verify("error", Duration::from_millis(1));
        record_settle("success", Duration::from_millis(1));
        record_settle("failure", Duration::from_millis(1));
        record_settle("error", Duration::from_millis(1));
    }
}
