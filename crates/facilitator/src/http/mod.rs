//! Axum router for facilitator protocol and ops routes.

mod auth;
mod classify;
mod ops;
mod protocol;

use std::sync::Arc;
use std::time::Duration;

use auth::require_bearer;
use axum::Router;
use axum::extract::DefaultBodyLimit;
use axum::http::{HeaderValue, StatusCode};
use axum::middleware;
use axum::routing::{get, post};
use ops::{get_healthz, get_readyz};
use protocol::{get_supported, post_settle, post_verify};
use r402_facilitator::DynFacilitator;
use tower_http::cors::{AllowOrigin, CorsLayer};
use tower_http::timeout::TimeoutLayer;
use tower_http::trace::TraceLayer;

use crate::config::HttpConfig;
use crate::error::Error;

/// `GET /supported` timeout. Not independently configurable.
const SUPPORTED_TIMEOUT: Duration = Duration::from_secs(30);
/// `/healthz` and `/readyz` timeout.
const OPS_TIMEOUT: Duration = Duration::from_secs(5);

/// Shared HTTP state.
#[derive(Clone)]
pub struct AppState {
    /// In-process scheme handlers.
    facilitator: Arc<dyn DynFacilitator>,
    /// Readiness: nonempty constructed map.
    ready: bool,
    /// Shared bearer for protocol routes. `None` = auth off.
    bearer: Option<Arc<str>>,
}

impl std::fmt::Debug for AppState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AppState")
            .field("ready", &self.ready)
            .field("auth", &self.bearer.is_some())
            .finish_non_exhaustive()
    }
}

impl AppState {
    /// Wrap a facilitator. Not ready; no bearer.
    #[must_use]
    pub fn new(facilitator: Arc<dyn DynFacilitator>) -> Self {
        Self {
            facilitator,
            ready: false,
            bearer: None,
        }
    }

    /// Mark `/readyz` as 200 when the process can serve kinds.
    #[must_use]
    pub const fn with_ready(mut self, ready: bool) -> Self {
        self.ready = ready;
        self
    }

    /// Require `Authorization: Bearer` on protocol routes.
    #[must_use]
    pub fn with_bearer(mut self, token: impl Into<Arc<str>>) -> Self {
        self.bearer = Some(token.into());
        self
    }
}

/// Protocol vs ops tower timeouts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HttpTimeouts {
    /// `POST /verify`.
    pub verify: Duration,
    /// `POST /settle`.
    pub settle: Duration,
    /// `GET /supported`.
    pub supported: Duration,
    /// `/healthz` and `/readyz`.
    pub ops: Duration,
}

impl Default for HttpTimeouts {
    fn default() -> Self {
        Self {
            verify: Duration::from_secs(30),
            settle: Duration::from_secs(30),
            supported: SUPPORTED_TIMEOUT,
            ops: OPS_TIMEOUT,
        }
    }
}

impl HttpTimeouts {
    /// Timeouts from `[http]`. Ops and `/supported` stay at process defaults.
    #[must_use]
    pub const fn from_http(http: &HttpConfig) -> Self {
        Self {
            verify: http.verify_timeout,
            settle: http.settle_timeout,
            supported: SUPPORTED_TIMEOUT,
            ops: OPS_TIMEOUT,
        }
    }

    /// SIGTERM drain: longest protocol timeout + 5s.
    #[must_use]
    pub fn drain(self) -> Duration {
        self.verify
            .max(self.settle)
            .max(self.supported)
            .saturating_add(Duration::from_secs(5))
    }
}

/// Protocol + ops router with default timeouts. No CORS.
pub fn router(state: AppState) -> Router {
    router_with_timeouts(state, HttpTimeouts::default())
}

/// Protocol + ops router with explicit timeouts. No CORS.
pub fn router_with_timeouts(state: AppState, timeouts: HttpTimeouts) -> Router {
    build_router(state, timeouts, default_body_limit_usize(), None)
}

/// Router from `[http]`, including CORS and body limit.
///
/// # Errors
///
/// Invalid CORS origin header value.
pub fn router_from_config(state: AppState, http: &HttpConfig) -> Result<Router, Error> {
    let cors = cors_layer(&http.cors_origins)?;
    Ok(build_router(
        state,
        HttpTimeouts::from_http(http),
        body_limit_usize(http.body_limit_bytes),
        cors,
    ))
}

fn build_router(
    state: AppState,
    timeouts: HttpTimeouts,
    body_limit: usize,
    cors: Option<CorsLayer>,
) -> Router {
    let protocol = protocol_routes(&state, timeouts);
    let ops = ops_routes(timeouts);
    let mut app = protocol
        .merge(ops)
        .layer(DefaultBodyLimit::max(body_limit))
        .with_state(state)
        .layer(TraceLayer::new_for_http());
    if let Some(layer) = cors {
        app = app.layer(layer);
    }
    app
}

fn protocol_routes(state: &AppState, timeouts: HttpTimeouts) -> Router<AppState> {
    Router::new()
        .merge(
            Router::new()
                .route("/verify", post(post_verify))
                .layer(timeout_layer(timeouts.verify)),
        )
        .merge(
            Router::new()
                .route("/settle", post(post_settle))
                .layer(timeout_layer(timeouts.settle)),
        )
        .merge(
            Router::new()
                .route("/supported", get(get_supported))
                .layer(timeout_layer(timeouts.supported)),
        )
        .layer(middleware::from_fn_with_state(
            state.clone(),
            require_bearer,
        ))
}

fn ops_routes(timeouts: HttpTimeouts) -> Router<AppState> {
    Router::new()
        .route("/healthz", get(get_healthz))
        .route("/readyz", get(get_readyz))
        .layer(timeout_layer(timeouts.ops))
}

fn timeout_layer(duration: Duration) -> TimeoutLayer {
    TimeoutLayer::with_status_code(StatusCode::GATEWAY_TIMEOUT, duration)
}

fn cors_layer(origins: &[String]) -> Result<Option<CorsLayer>, Error> {
    if origins.is_empty() {
        return Ok(None);
    }
    let mut parsed = Vec::with_capacity(origins.len());
    for origin in origins {
        let value = HeaderValue::from_str(origin)
            .map_err(|err| Error::config_with(format!("invalid CORS origin '{origin}'"), err))?;
        parsed.push(value);
    }
    Ok(Some(
        CorsLayer::new()
            .allow_origin(AllowOrigin::list(parsed))
            .allow_methods([axum::http::Method::GET, axum::http::Method::POST])
            .allow_headers([
                axum::http::header::AUTHORIZATION,
                axum::http::header::CONTENT_TYPE,
            ]),
    ))
}

fn default_body_limit_usize() -> usize {
    body_limit_usize(HttpConfig::default().body_limit_bytes)
}

fn body_limit_usize(bytes: u64) -> usize {
    usize::try_from(bytes).unwrap_or(usize::MAX)
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "unit tests"
)]
mod tests {
    use std::future::Future;

    use axum::body::Body;
    use axum::http::Request;
    use http_body_util::BodyExt;
    use r402_facilitator::Facilitator;
    use r402_protocol::error::FacilitatorError;
    use r402_protocol::payment::{
        SettleRequest, SettleResponse, SupportedResponse, VerifyRequest, VerifyResponse,
    };
    use tower::ServiceExt;

    use super::*;

    struct Hang;

    impl Facilitator for Hang {
        fn verify(
            &self,
            _request: VerifyRequest,
        ) -> impl Future<Output = Result<VerifyResponse, FacilitatorError>> + Send {
            std::future::pending()
        }

        fn settle(
            &self,
            _request: SettleRequest,
        ) -> impl Future<Output = Result<SettleResponse, FacilitatorError>> + Send {
            std::future::pending()
        }

        fn supported(
            &self,
        ) -> impl Future<Output = Result<SupportedResponse, FacilitatorError>> + Send {
            std::future::ready(Ok(SupportedResponse::new()))
        }
    }

    fn verify_body() -> Body {
        Body::from(
            r#"{"x402Version":2,"paymentPayload":{"accepted":{"network":"eip155:84532","scheme":"exact"}},"paymentRequirements":{"network":"eip155:84532"}}"#,
        )
    }

    #[tokio::test(start_paused = true)]
    async fn protocol_timeout_is_not_ops_timeout() {
        let timeouts = HttpTimeouts {
            verify: Duration::from_millis(100),
            settle: Duration::from_millis(100),
            supported: Duration::from_millis(100),
            ops: Duration::from_millis(10),
        };
        let app = router_with_timeouts(AppState::new(Arc::new(Hang)), timeouts);
        let req = Request::builder()
            .method("POST")
            .uri("/verify")
            .header("content-type", "application/json")
            .body(verify_body())
            .expect("request");
        let handle = tokio::spawn(async move { app.oneshot(req).await });
        tokio::task::yield_now().await;
        tokio::time::advance(Duration::from_millis(20)).await;
        assert!(
            !handle.is_finished(),
            "ops timeout must not fire on /verify"
        );
        tokio::time::advance(Duration::from_millis(100)).await;
        let response = handle.await.expect("join").expect("oneshot");
        assert_eq!(
            response.status(),
            StatusCode::GATEWAY_TIMEOUT,
            "protocol timeout"
        );
        let bytes = response
            .into_body()
            .collect()
            .await
            .expect("body")
            .to_bytes();
        assert!(bytes.is_empty(), "504 body is empty");
    }

    #[tokio::test(start_paused = true)]
    async fn ops_timeout_layer_returns_504() {
        async fn hang() -> &'static str {
            std::future::pending::<()>().await;
            "ok"
        }
        let app = Router::new()
            .route("/healthz", get(hang))
            .layer(timeout_layer(Duration::from_millis(10)));
        let req = Request::builder()
            .uri("/healthz")
            .body(Body::empty())
            .expect("request");
        let handle = tokio::spawn(async move { app.oneshot(req).await });
        tokio::task::yield_now().await;
        tokio::time::advance(Duration::from_millis(20)).await;
        let response = handle.await.expect("join").expect("oneshot");
        assert_eq!(
            response.status(),
            StatusCode::GATEWAY_TIMEOUT,
            "ops timeout"
        );
    }

    #[test]
    fn drain_covers_longest_protocol_timeout() {
        let http = HttpConfig {
            verify_timeout: Duration::from_mins(1),
            settle_timeout: Duration::from_secs(30),
            ..HttpConfig::default()
        };
        assert_eq!(
            HttpTimeouts::from_http(&http).drain(),
            Duration::from_secs(65),
            "verify 60s + 5s slack"
        );
    }
}
