//! HTTP wire tests for protocol, ops, auth, timeouts, and metrics routing.

#![allow(
    unused_crate_dependencies,
    reason = "integration tests link the package graph"
)]
#![allow(
    clippy::tests_outside_test_module,
    reason = "integration test binaries put #[test] fns at file scope"
)]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "idiomatic test-code patterns"
)]

use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use facilitator::{
    AppState, FacilitatorMap, HttpConfig, HttpTimeouts, MetricsHandle, metrics_router, router,
    router_from_config, router_with_timeouts,
};
use http_body_util::BodyExt;
use r402_facilitator::Facilitator;
use r402_protocol::error::{FacilitatorError, FacilitatorTransportKind};
use r402_protocol::payment::{
    SettleRequest, SettleResponse, SupportedResponse, VerifyRequest, VerifyResponse,
};
use serde_json::Value;
use tower::ServiceExt;

fn app() -> axum::Router {
    router(AppState::new(Arc::new(FacilitatorMap::new())))
}

fn v2_body() -> Body {
    Body::from(
        r#"{"x402Version":2,"paymentPayload":{"accepted":{"network":"eip155:84532","scheme":"exact"}},"paymentRequirements":{"network":"eip155:84532"}}"#,
    )
}

async fn send(app: axum::Router, req: Request<Body>) -> (StatusCode, Value) {
    let response = app.oneshot(req).await.expect("oneshot");
    let status = response.status();
    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("body")
        .to_bytes();
    if bytes.is_empty() {
        return (status, Value::Null);
    }
    let json = serde_json::from_slice(&bytes)
        .unwrap_or_else(|_| serde_json::json!({ "raw": bytes.len() }));
    (status, json)
}

fn json_req(method: &str, uri: &str, body: Body) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json")
        .body(body)
        .unwrap()
}

#[tokio::test]
async fn supported_empty_map_returns_empty_kinds() {
    let req = Request::builder()
        .method("GET")
        .uri("/supported")
        .body(Body::empty())
        .unwrap();
    let (status, json) = send(app(), req).await;
    assert_eq!(status, StatusCode::OK, "always 200");
    let kinds = json["kinds"].as_array().expect("kinds");
    assert!(kinds.is_empty(), "stub map has no kinds");
    assert_eq!(json["extensions"], serde_json::json!([]), "extensions");
}

#[tokio::test]
async fn get_root_is_404() {
    let req = Request::builder()
        .method("GET")
        .uri("/")
        .body(Body::empty())
        .unwrap();
    let (status, _) = send(app(), req).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "GET / removed");
}

struct FailingSupported;

impl Facilitator for FailingSupported {
    fn verify(
        &self,
        _request: VerifyRequest,
    ) -> impl Future<Output = Result<VerifyResponse, FacilitatorError>> + Send {
        std::future::ready(Err(FacilitatorError::aborted("test", "verify unused")))
    }

    fn settle(
        &self,
        _request: SettleRequest,
    ) -> impl Future<Output = Result<SettleResponse, FacilitatorError>> + Send {
        std::future::ready(Err(FacilitatorError::aborted("test", "settle unused")))
    }

    fn supported(
        &self,
    ) -> impl Future<Output = Result<SupportedResponse, FacilitatorError>> + Send {
        std::future::ready(Err(FacilitatorError::aborted(
            "test",
            "supported aggregation failed",
        )))
    }
}

#[tokio::test]
async fn supported_handler_error_is_500() {
    let app = router(AppState::new(Arc::new(FailingSupported)));
    let req = Request::builder()
        .method("GET")
        .uri("/supported")
        .body(Body::empty())
        .unwrap();
    let (status, json) = send(app, req).await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR, "aggregation err");
    assert_eq!(json, Value::Null, "no synthesized empty kinds body");
}

#[tokio::test]
async fn get_health_is_404() {
    let req = Request::builder()
        .method("GET")
        .uri("/health")
        .body(Body::empty())
        .unwrap();
    let (status, _) = send(app(), req).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "GET /health removed");
}

#[tokio::test]
async fn healthz_is_ok() {
    let req = Request::builder()
        .method("GET")
        .uri("/healthz")
        .body(Body::empty())
        .unwrap();
    let (status, json) = send(app(), req).await;
    assert_eq!(status, StatusCode::OK, "liveness");
    assert_eq!(json["status"], "ok", "body");
}

#[tokio::test]
async fn readyz_empty_map_is_503() {
    let req = Request::builder()
        .method("GET")
        .uri("/readyz")
        .body(Body::empty())
        .unwrap();
    let (status, json) = send(app(), req).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "not ready");
    assert_eq!(json["status"], "not ready", "body");
}

#[tokio::test]
async fn readyz_ready_is_200() {
    let app = router(AppState::new(Arc::new(FacilitatorMap::new())).with_ready(true));
    let req = Request::builder()
        .method("GET")
        .uri("/readyz")
        .body(Body::empty())
        .unwrap();
    let (status, json) = send(app, req).await;
    assert_eq!(status, StatusCode::OK, "ready");
    assert_eq!(json["status"], "ok", "body");
}

#[tokio::test]
async fn envelope_wrong_version_is_200_invalid_x402_version() {
    let body = Body::from(r#"{"x402Version":1,"paymentPayload":{},"paymentRequirements":{}}"#);
    let (status, json) = send(app(), json_req("POST", "/verify", body)).await;
    assert_eq!(status, StatusCode::OK, "protocol JSON");
    assert_eq!(json["isValid"], false, "invalid");
    assert_eq!(json["invalidReason"], "invalid_x402_version", "reason");
}

#[tokio::test]
async fn envelope_v2_bad_accepted_is_200_invalid_payload() {
    let body = Body::from(
        r#"{"x402Version":2,"paymentPayload":{},"paymentRequirements":{"network":"eip155:84532"}}"#,
    );
    let (status, json) = send(app(), json_req("POST", "/verify", body)).await;
    assert_eq!(status, StatusCode::OK, "protocol JSON");
    assert_eq!(json["isValid"], false, "invalid");
    assert_eq!(json["invalidReason"], "invalid_payload", "reason");
}

#[tokio::test]
async fn envelope_settle_wrong_version_is_200_failure() {
    let body = Body::from(r#"{"x402Version":1,"paymentPayload":{},"paymentRequirements":{}}"#);
    let (status, json) = send(app(), json_req("POST", "/settle", body)).await;
    assert_eq!(status, StatusCode::OK, "protocol JSON");
    assert_eq!(json["success"], false, "failure");
    assert_eq!(json["errorReason"], "invalid_x402_version", "reason");
}

#[tokio::test]
async fn unparseable_json_is_400() {
    let (status, json) = send(app(), json_req("POST", "/verify", Body::from("not-json"))).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "JsonRejection");
    assert_eq!(json["error"], "invalid request body", "body");
}

struct AlwaysValid;

impl Facilitator for AlwaysValid {
    fn verify(
        &self,
        _request: VerifyRequest,
    ) -> impl Future<Output = Result<VerifyResponse, FacilitatorError>> + Send {
        std::future::ready(Ok(VerifyResponse::valid("0xabc")))
    }

    fn settle(
        &self,
        _request: SettleRequest,
    ) -> impl Future<Output = Result<SettleResponse, FacilitatorError>> + Send {
        std::future::ready(Err(FacilitatorError::aborted("test", "settle unused")))
    }

    fn supported(
        &self,
    ) -> impl Future<Output = Result<SupportedResponse, FacilitatorError>> + Send {
        std::future::ready(Ok(SupportedResponse::new()))
    }
}

#[tokio::test]
async fn well_formed_verify_is_200_valid() {
    let app = router(AppState::new(Arc::new(AlwaysValid)));
    let (status, json) = send(app, json_req("POST", "/verify", v2_body())).await;
    assert_eq!(status, StatusCode::OK, "protocol JSON");
    assert_eq!(json["isValid"], true, "valid");
}

struct TransportFail;

impl Facilitator for TransportFail {
    fn verify(
        &self,
        _request: VerifyRequest,
    ) -> impl Future<Output = Result<VerifyResponse, FacilitatorError>> + Send {
        std::future::ready(Err(FacilitatorError::transport(
            FacilitatorTransportKind::Io,
        )))
    }

    fn settle(
        &self,
        _request: SettleRequest,
    ) -> impl Future<Output = Result<SettleResponse, FacilitatorError>> + Send {
        std::future::ready(Err(FacilitatorError::transport(
            FacilitatorTransportKind::Io,
        )))
    }

    fn supported(
        &self,
    ) -> impl Future<Output = Result<SupportedResponse, FacilitatorError>> + Send {
        std::future::ready(Ok(SupportedResponse::new()))
    }
}

#[tokio::test]
async fn transport_error_is_502() {
    let app = router(AppState::new(Arc::new(TransportFail)));
    let (status, json) = send(app, json_req("POST", "/verify", v2_body())).await;
    assert_eq!(status, StatusCode::BAD_GATEWAY, "Transport");
    assert_eq!(json["error"], "facilitator transport", "body");
}

#[tokio::test]
async fn settle_transport_error_is_502() {
    let app = router(AppState::new(Arc::new(TransportFail)));
    let (status, json) = send(app, json_req("POST", "/settle", v2_body())).await;
    assert_eq!(status, StatusCode::BAD_GATEWAY, "Transport");
    assert_eq!(json["error"], "facilitator transport", "body");
}

#[tokio::test]
async fn bearer_missing_on_supported_is_401() {
    let app = router(AppState::new(Arc::new(FacilitatorMap::new())).with_bearer("secret"));
    let req = Request::builder()
        .method("GET")
        .uri("/supported")
        .body(Body::empty())
        .unwrap();
    let (status, json) = send(app, req).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "missing bearer");
    assert_eq!(json["error"], "unauthorized", "body");
}

#[tokio::test]
async fn bearer_wrong_token_is_401() {
    let app = router(AppState::new(Arc::new(FacilitatorMap::new())).with_bearer("secret"));
    let req = Request::builder()
        .method("GET")
        .uri("/supported")
        .header("authorization", "Bearer other")
        .body(Body::empty())
        .unwrap();
    let (status, json) = send(app, req).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "wrong bearer");
    assert_eq!(json["error"], "unauthorized", "body");
}

#[tokio::test]
async fn bearer_correct_token_allows_supported() {
    let app = router(AppState::new(Arc::new(FacilitatorMap::new())).with_bearer("secret"));
    let req = Request::builder()
        .method("GET")
        .uri("/supported")
        .header("authorization", "Bearer secret")
        .body(Body::empty())
        .unwrap();
    let (status, json) = send(app, req).await;
    assert_eq!(status, StatusCode::OK, "authorized");
    assert!(json["kinds"].as_array().expect("kinds").is_empty(), "kinds");
}

#[tokio::test]
async fn bearer_scheme_is_case_insensitive() {
    let app = router(AppState::new(Arc::new(FacilitatorMap::new())).with_bearer("secret"));
    let req = Request::builder()
        .method("GET")
        .uri("/supported")
        .header("authorization", "bearer secret")
        .body(Body::empty())
        .unwrap();
    let (status, _) = send(app, req).await;
    assert_eq!(status, StatusCode::OK, "lowercase bearer");
}

#[tokio::test]
async fn bearer_does_not_protect_healthz() {
    let app = router(AppState::new(Arc::new(FacilitatorMap::new())).with_bearer("secret"));
    let req = Request::builder()
        .method("GET")
        .uri("/healthz")
        .body(Body::empty())
        .unwrap();
    let (status, json) = send(app, req).await;
    assert_eq!(status, StatusCode::OK, "ops unauthenticated");
    assert_eq!(json["status"], "ok", "body");
}

#[tokio::test]
async fn bearer_protects_verify() {
    let app = router(AppState::new(Arc::new(AlwaysValid)).with_bearer("secret"));
    let (status, json) = send(app, json_req("POST", "/verify", v2_body())).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "verify requires bearer");
    assert_eq!(json["error"], "unauthorized", "body");
}

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

#[tokio::test(start_paused = true)]
async fn timeout_layer_protocol_vs_ops() {
    let timeouts = HttpTimeouts {
        verify: Duration::from_millis(100),
        settle: Duration::from_millis(100),
        supported: Duration::from_millis(100),
        ops: Duration::from_millis(10),
    };
    let app = router_with_timeouts(AppState::new(Arc::new(Hang)), timeouts);
    let health = app.clone();
    let health_req = Request::builder()
        .method("GET")
        .uri("/healthz")
        .body(Body::empty())
        .unwrap();
    let (health_status, _) = send(health, health_req).await;
    assert_eq!(health_status, StatusCode::OK, "ops is not hanging");

    let req = json_req("POST", "/verify", v2_body());
    let handle = tokio::spawn(async move { send(app, req).await });
    tokio::task::yield_now().await;
    tokio::time::advance(Duration::from_millis(20)).await;
    assert!(
        !handle.is_finished(),
        "ops timeout must not fire on protocol"
    );
    tokio::time::advance(Duration::from_millis(100)).await;
    let (status, json) = handle.await.expect("join");
    assert_eq!(status, StatusCode::GATEWAY_TIMEOUT, "protocol timeout");
    assert_eq!(json, Value::Null, "empty 504 body");
}

#[tokio::test]
async fn metrics_not_on_protocol_port() {
    let req = Request::builder()
        .method("GET")
        .uri("/metrics")
        .body(Body::empty())
        .unwrap();
    let (status, _) = send(app(), req).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "/metrics not on protocol");
}

#[tokio::test]
async fn metrics_listen_exposes_metrics() {
    let app = metrics_router(MetricsHandle::disabled());
    let req = Request::builder()
        .method("GET")
        .uri("/metrics")
        .body(Body::empty())
        .unwrap();
    let (status, _) = send(app, req).await;
    assert_eq!(status, StatusCode::OK, "metrics listen");
}

async fn acao(app: axum::Router, origin: &str) -> Option<String> {
    let req = Request::builder()
        .method("GET")
        .uri("/healthz")
        .header("origin", origin)
        .body(Body::empty())
        .unwrap();
    let response = app.oneshot(req).await.expect("oneshot");
    response
        .headers()
        .get("access-control-allow-origin")
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned)
}

#[tokio::test]
async fn cors_empty_allowlist_omits_acao() {
    let http = HttpConfig::default();
    assert!(http.cors_origins.is_empty(), "default empty");
    let app =
        router_from_config(AppState::new(Arc::new(FacilitatorMap::new())), &http).expect("router");
    assert_eq!(
        acao(app, "https://evil.example").await,
        None,
        "no CORS layer"
    );
}

#[tokio::test]
async fn cors_allowlist_echoes_allowed_origin() {
    let http = HttpConfig {
        cors_origins: vec!["https://app.example".to_owned()],
        ..HttpConfig::default()
    };
    let app =
        router_from_config(AppState::new(Arc::new(FacilitatorMap::new())), &http).expect("router");
    assert_eq!(
        acao(app, "https://app.example").await.as_deref(),
        Some("https://app.example"),
        "allowlisted origin"
    );
}

#[tokio::test]
async fn cors_allowlist_omits_disallowed_origin() {
    let http = HttpConfig {
        cors_origins: vec!["https://app.example".to_owned()],
        ..HttpConfig::default()
    };
    let app =
        router_from_config(AppState::new(Arc::new(FacilitatorMap::new())), &http).expect("router");
    assert_eq!(
        acao(app, "https://evil.example").await,
        None,
        "disallowed origin"
    );
}

#[tokio::test]
async fn cors_preflight_allows_authorization_and_content_type() {
    let http = HttpConfig {
        cors_origins: vec!["https://app.example".to_owned()],
        ..HttpConfig::default()
    };
    let app =
        router_from_config(AppState::new(Arc::new(FacilitatorMap::new())), &http).expect("router");
    let req = Request::builder()
        .method("OPTIONS")
        .uri("/verify")
        .header("origin", "https://app.example")
        .header("access-control-request-method", "POST")
        .header(
            "access-control-request-headers",
            "authorization,content-type",
        )
        .body(Body::empty())
        .unwrap();
    let response = app.oneshot(req).await.expect("oneshot");
    assert_eq!(response.status(), StatusCode::OK, "preflight");
    let headers = response.headers();
    let origin = headers
        .get("access-control-allow-origin")
        .and_then(|v| v.to_str().ok());
    assert_eq!(origin, Some("https://app.example"), "origin");
    let allow_headers = headers
        .get("access-control-allow-headers")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_ascii_lowercase();
    assert!(
        allow_headers.contains("authorization"),
        "authorization: {allow_headers}"
    );
    assert!(
        allow_headers.contains("content-type"),
        "content-type: {allow_headers}"
    );
    let allow_methods = headers
        .get("access-control-allow-methods")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_ascii_lowercase();
    assert!(allow_methods.contains("post"), "POST: {allow_methods}");
    assert!(allow_methods.contains("get"), "GET: {allow_methods}");
}
