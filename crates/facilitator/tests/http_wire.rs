//! HTTP wire tests for `GET /supported`.

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

use axum::body::Body;
use axum::http::{Request, StatusCode};
use facilitator::{AppState, FacilitatorMap, router};
use http_body_util::BodyExt;
use r402_facilitator::Facilitator;
use r402_protocol::error::FacilitatorError;
use r402_protocol::payment::{
    SettleRequest, SettleResponse, SupportedResponse, VerifyRequest, VerifyResponse,
};
use serde_json::Value;
use tower::ServiceExt;

fn app() -> axum::Router {
    router(AppState::new(Arc::new(FacilitatorMap::new())))
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
