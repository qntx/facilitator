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

//! Spec §7 HTTP wire tests against `routes()` with a stub facilitator.

use std::future::Future;
use std::path::PathBuf;
use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use facilitator::{FacilitatorState, routes};
use http_body_util::BodyExt;
use r402_core::error::{ErrorReason, FacilitatorError};
use r402_core::facilitator::Facilitator;
use r402_core::scheme::SchemeRegistry;
use r402_core::wire::{
    Extensions, SettleRequest, SettleResponse, SupportedPaymentKind, SupportedResponse,
    VerifyRequest, VerifyResponse,
};
use serde_json::{Value, json};
use tower::ServiceExt;

const PAYER: &str = "0x857b06519E91e3A54538791bDbb0E22373e36b66";

struct StubFacilitator;

impl Facilitator for StubFacilitator {
    fn verify(
        &self,
        _request: VerifyRequest,
    ) -> impl Future<Output = Result<VerifyResponse, FacilitatorError>> + Send {
        std::future::ready(Ok(VerifyResponse::valid(PAYER)))
    }

    fn settle(
        &self,
        request: SettleRequest,
    ) -> impl Future<Output = Result<SettleResponse, FacilitatorError>> + Send {
        std::future::ready(Ok(SettleResponse::Success {
            payer: PAYER.into(),
            transaction: "0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef"
                .into(),
            network: request.network().into(),
            amount: None,
            extensions: Extensions::new(),
        }))
    }

    fn supported(
        &self,
    ) -> impl Future<Output = Result<SupportedResponse, FacilitatorError>> + Send {
        std::future::ready(Ok(SupportedResponse::new().with_kinds(vec![
            SupportedPaymentKind::new(2, "exact", "eip155:84532"),
            SupportedPaymentKind::new(2, "exact", "solana:EtWTRABZaYq6iMfeYKouRu166VU2xqa1")
                .with_extra(json!({
                    "feePayer": "CKPKJWNdJEqa81x7CkZ14BVPiY6y16Sxs7owznqtWYp5"
                })),
        ])))
    }
}

struct FailingSettle;

impl Facilitator for FailingSettle {
    fn verify(
        &self,
        _request: VerifyRequest,
    ) -> impl Future<Output = Result<VerifyResponse, FacilitatorError>> + Send {
        std::future::ready(Ok(VerifyResponse::valid(PAYER)))
    }

    fn settle(
        &self,
        _request: SettleRequest,
    ) -> impl Future<Output = Result<SettleResponse, FacilitatorError>> + Send {
        std::future::ready(Ok(SettleResponse::Failure {
            reason: ErrorReason::InsufficientFunds,
            message: None,
            payer: Some(PAYER.into()),
            network: "eip155:84532".into(),
            extensions: Extensions::new(),
        }))
    }

    fn supported(
        &self,
    ) -> impl Future<Output = Result<SupportedResponse, FacilitatorError>> + Send {
        std::future::ready(Ok(SupportedResponse::new()))
    }
}

struct ErroringFacilitator;

impl Facilitator for ErroringFacilitator {
    fn verify(
        &self,
        _request: VerifyRequest,
    ) -> impl Future<Output = Result<VerifyResponse, FacilitatorError>> + Send {
        std::future::ready(Err(FacilitatorError::Onchain("rpc down".into())))
    }

    fn settle(
        &self,
        _request: SettleRequest,
    ) -> impl Future<Output = Result<SettleResponse, FacilitatorError>> + Send {
        std::future::ready(Err(FacilitatorError::Onchain("rpc down".into())))
    }

    fn supported(
        &self,
    ) -> impl Future<Output = Result<SupportedResponse, FacilitatorError>> + Send {
        std::future::ready(Ok(SupportedResponse::new()))
    }
}

struct InvalidVerify;

impl Facilitator for InvalidVerify {
    fn verify(
        &self,
        _request: VerifyRequest,
    ) -> impl Future<Output = Result<VerifyResponse, FacilitatorError>> + Send {
        std::future::ready(Ok(VerifyResponse::invalid(
            None,
            ErrorReason::InsufficientFunds,
        )))
    }

    fn settle(
        &self,
        _request: SettleRequest,
    ) -> impl Future<Output = Result<SettleResponse, FacilitatorError>> + Send {
        std::future::ready(Err(FacilitatorError::Onchain("unused".into())))
    }

    fn supported(
        &self,
    ) -> impl Future<Output = Result<SupportedResponse, FacilitatorError>> + Send {
        std::future::ready(Ok(SupportedResponse::new()))
    }
}

fn stub_app() -> axum::Router {
    let state: FacilitatorState = Arc::new(StubFacilitator);
    routes().with_state(state)
}

fn fixture(name: &str) -> Value {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("tests/fixtures/spec_v2");
    path.push(name);
    let bytes = std::fs::read(&path).unwrap_or_else(|err| panic!("{}: {err}", path.display()));
    serde_json::from_slice(&bytes)
        .unwrap_or_else(|err| panic!("invalid JSON {}: {err}", path.display()))
}

fn assert_roundtrip<T>(value: &Value)
where
    T: serde::Serialize + serde::de::DeserializeOwned,
{
    let parsed: T = serde_json::from_value(value.clone()).expect("fixture deserialises");
    let again = serde_json::to_value(&parsed).expect("re-serialises");
    assert_eq!(&again, value, "fixture round-trip diverged");
}

fn ts_client_body() -> Value {
    json!({
        "x402Version": 2,
        "paymentPayload": fixture("payment_payload_eip3009.json"),
        "paymentRequirements": {
            "scheme": "exact",
            "network": "eip155:84532",
            "amount": "10000",
            "asset": "0x036CbD53842c5426634e7929541eC2318f3dCF7e",
            "payTo": "0x209693Bc6afc0C5328bA36FaF03C514EF312287C",
            "maxTimeoutSeconds": 60
        }
    })
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
    let json = serde_json::from_slice(&bytes).unwrap_or_else(|_| json!({ "raw": bytes.len() }));
    (status, json)
}

fn json_request(method: &str, uri: &str, body: &Value) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(body).unwrap()))
        .unwrap()
}

#[test]
fn verify_response_valid_fixture_roundtrips() {
    assert_roundtrip::<VerifyResponse>(&fixture("verify_response_valid.json"));
}

#[test]
fn verify_response_invalid_fixture_roundtrips() {
    assert_roundtrip::<VerifyResponse>(&fixture("verify_response_invalid.json"));
}

#[test]
fn settle_response_success_fixture_roundtrips() {
    assert_roundtrip::<SettleResponse>(&fixture("settle_response_success.json"));
}

#[test]
fn settle_response_failure_fixture_roundtrips() {
    assert_roundtrip::<SettleResponse>(&fixture("settle_response_failure.json"));
}

#[test]
fn supported_response_fixture_roundtrips() {
    assert_roundtrip::<SupportedResponse>(&fixture("supported_response.json"));
}

#[test]
fn settle_failure_serializes_empty_transaction() {
    let json = fixture("settle_response_failure.json");
    assert_eq!(json["transaction"], "", "spec requires empty string");
    let parsed: SettleResponse = serde_json::from_value(json).unwrap();
    let encoded = serde_json::to_value(&parsed).unwrap();
    assert_eq!(encoded["transaction"], "", "r402 Failure keeps transaction");
}

#[test]
fn ts_client_body_deserializes() {
    let req: VerifyRequest = serde_json::from_value(ts_client_body()).unwrap();
    assert!(req.scheme_slug().is_some(), "accepted is well-formed");
}

#[tokio::test]
async fn ts_client_body_accepted_by_verify() {
    let (status, json) = send(
        stub_app(),
        json_request("POST", "/verify", &ts_client_body()),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "protocol outcome is 200");
    assert_eq!(json["isValid"], true, "stub returns valid");
}

#[tokio::test]
async fn get_root_is_404() {
    let req = Request::builder()
        .method("GET")
        .uri("/")
        .body(Body::empty())
        .unwrap();
    let (status, _) = send(stub_app(), req).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "GET / removed");
}

#[tokio::test]
async fn x402_version_1_is_invalid_version() {
    let body = json!({
        "x402Version": 1,
        "paymentPayload": { "accepted": { "network": "eip155:84532", "scheme": "exact" } },
        "paymentRequirements": { "network": "eip155:84532" }
    });
    let (status, json) = send(stub_app(), json_request("POST", "/verify", &body)).await;
    assert_eq!(status, StatusCode::OK, "200");
    assert_eq!(json["isValid"], false, "invalid");
    assert_eq!(json["invalidReason"], "invalid_x402_version", "version");
}

#[tokio::test]
async fn empty_object_is_invalid_version() {
    let (status, json) = send(stub_app(), json_request("POST", "/verify", &json!({}))).await;
    assert_eq!(status, StatusCode::OK, "200");
    assert_eq!(
        json["invalidReason"], "invalid_x402_version",
        "empty object"
    );
}

#[tokio::test]
async fn version_2_without_accepted_is_invalid_payload() {
    let (status, json) = send(
        stub_app(),
        json_request("POST", "/verify", &json!({ "x402Version": 2 })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "200");
    assert_eq!(json["isValid"], false, "invalid");
    assert_eq!(json["invalidReason"], "invalid_payload", "no accepted");
}

#[tokio::test]
async fn settle_empty_object_is_invalid_version_with_empty_transaction() {
    let (status, json) = send(stub_app(), json_request("POST", "/settle", &json!({}))).await;
    assert_eq!(status, StatusCode::OK, "200");
    assert_eq!(json["success"], false, "failure");
    assert_eq!(json["errorReason"], "invalid_x402_version", "version");
    assert_eq!(json["transaction"], "", "required empty string");
}

#[tokio::test]
async fn non_json_body_is_400() {
    let req = Request::builder()
        .method("POST")
        .uri("/verify")
        .header("content-type", "application/json")
        .body(Body::from("not-json"))
        .unwrap();
    let (status, json) = send(stub_app(), req).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "400");
    assert_eq!(json["error"], "invalid request body", "message");
}

#[tokio::test]
async fn supported_kinds_are_v2_exact() {
    let req = Request::builder()
        .method("GET")
        .uri("/supported")
        .body(Body::empty())
        .unwrap();
    let (status, json) = send(stub_app(), req).await;
    assert_eq!(status, StatusCode::OK, "always 200");
    let kinds = json["kinds"].as_array().expect("kinds");
    assert!(!kinds.is_empty(), "stub advertises exact");
    assert_eq!(kinds[0]["x402Version"], 2, "v2");
    assert_eq!(kinds[0]["scheme"], "exact", "exact");
}

#[tokio::test]
async fn supported_preserves_solana_network_and_fee_payer_extra() {
    let req = Request::builder()
        .method("GET")
        .uri("/supported")
        .body(Body::empty())
        .unwrap();
    let (status, json) = send(stub_app(), req).await;
    assert_eq!(status, StatusCode::OK, "always 200");
    let kinds = json["kinds"].as_array().expect("kinds");
    let solana = kinds
        .iter()
        .find(|k| k["network"] == "solana:EtWTRABZaYq6iMfeYKouRu166VU2xqa1")
        .expect("solana CAIP-2 kind");
    assert_eq!(solana["scheme"], "exact", "exact");
    assert_eq!(solana["x402Version"], 2, "v2");
    assert_eq!(
        solana["extra"]["feePayer"], "CKPKJWNdJEqa81x7CkZ14BVPiY6y16Sxs7owznqtWYp5",
        "clients need extra.feePayer to build the tx"
    );
}

#[tokio::test]
async fn health_ok() {
    let req = Request::builder()
        .method("GET")
        .uri("/health")
        .body(Body::empty())
        .unwrap();
    let (status, json) = send(stub_app(), req).await;
    assert_eq!(status, StatusCode::OK, "200");
    assert_eq!(json["status"], "ok", "liveness");
}

#[tokio::test]
async fn settle_failure_http_includes_empty_transaction() {
    let state: FacilitatorState = Arc::new(FailingSettle);
    let app = routes().with_state(state);
    let (status, json) = send(app, json_request("POST", "/settle", &ts_client_body())).await;
    assert_eq!(status, StatusCode::OK, "200");
    assert_eq!(json["success"], false, "failure");
    assert_eq!(json["transaction"], "", "empty on failure");
    assert_eq!(json["errorReason"], "insufficient_funds", "reason");
}

#[tokio::test]
async fn well_formed_unknown_network_is_no_facilitator() {
    let state: FacilitatorState = Arc::new(SchemeRegistry::new());
    let app = routes().with_state(state);
    let (status, json) = send(app, json_request("POST", "/verify", &ts_client_body())).await;
    assert_eq!(status, StatusCode::OK, "200");
    assert_eq!(json["isValid"], false, "invalid");
    assert_eq!(
        json["invalidReason"], "no_facilitator_for_network",
        "registry abort"
    );
}

#[tokio::test]
async fn facilitator_error_on_verify_is_200_invalid() {
    let state: FacilitatorState = Arc::new(ErroringFacilitator);
    let app = routes().with_state(state);
    let (status, json) = send(app, json_request("POST", "/verify", &ts_client_body())).await;
    assert_eq!(status, StatusCode::OK, "protocol error is still 200");
    assert_eq!(json["isValid"], false, "mapped to Invalid");
}

#[tokio::test]
async fn facilitator_error_on_settle_is_200_failure() {
    let state: FacilitatorState = Arc::new(ErroringFacilitator);
    let app = routes().with_state(state);
    let (status, json) = send(app, json_request("POST", "/settle", &ts_client_body())).await;
    assert_eq!(status, StatusCode::OK, "protocol error is still 200");
    assert_eq!(json["success"], false, "mapped to Failure");
    assert_eq!(json["transaction"], "", "empty on failure");
}

#[tokio::test]
async fn ok_invalid_verify_is_insufficient_funds() {
    let state: FacilitatorState = Arc::new(InvalidVerify);
    let app = routes().with_state(state);
    let (status, json) = send(app, json_request("POST", "/verify", &ts_client_body())).await;
    assert_eq!(status, StatusCode::OK, "200");
    assert_eq!(json["isValid"], false, "invalid");
    assert_eq!(json["invalidReason"], "insufficient_funds", "scheme reject");
}

#[cfg(feature = "metrics")]
mod recorded {
    use metrics_util::debugging::{DebugValue, DebuggingRecorder, Snapshotter};

    use super::*;

    fn assert_counter(snapshotter: &Snapshotter, name: &str, result: &str, n: u64) {
        let rows = snapshotter.snapshot().into_vec();
        let got = rows.iter().find_map(|(ck, _, _, value)| {
            if ck.key().name() != name {
                return None;
            }
            let labeled = ck
                .key()
                .labels()
                .any(|label| label.key() == "result" && label.value() == result);
            match (labeled, value) {
                (true, DebugValue::Counter(count)) => Some(*count),
                _ => None,
            }
        });
        assert_eq!(got, Some(n), "{name} result={result} in {rows:?}");
        assert!(
            rows.iter()
                .all(|(ck, _, _, _)| !ck.key().name().starts_with("r402_facilitator_")),
            "r402_facilitator_* must stay unused: {rows:?}"
        );
    }

    #[tokio::test]
    async fn verify_valid() {
        let recorder = DebuggingRecorder::new();
        let snapshotter = recorder.snapshotter();
        let _guard = metrics::set_default_local_recorder(&recorder);
        let (status, _) = send(
            stub_app(),
            json_request("POST", "/verify", &ts_client_body()),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "200");
        assert_counter(&snapshotter, "facilitator_http_verify_total", "valid", 1);
    }

    #[tokio::test]
    async fn verify_json_rejection_is_error() {
        let recorder = DebuggingRecorder::new();
        let snapshotter = recorder.snapshotter();
        let _guard = metrics::set_default_local_recorder(&recorder);
        let req = Request::builder()
            .method("POST")
            .uri("/verify")
            .header("content-type", "application/json")
            .body(Body::from("not-json"))
            .unwrap();
        let (status, _) = send(stub_app(), req).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "400");
        assert_counter(&snapshotter, "facilitator_http_verify_total", "error", 1);
    }

    #[tokio::test]
    async fn verify_envelope_is_invalid() {
        let recorder = DebuggingRecorder::new();
        let snapshotter = recorder.snapshotter();
        let _guard = metrics::set_default_local_recorder(&recorder);
        let (status, _) = send(stub_app(), json_request("POST", "/verify", &json!({}))).await;
        assert_eq!(status, StatusCode::OK, "200");
        assert_counter(&snapshotter, "facilitator_http_verify_total", "invalid", 1);
    }

    #[tokio::test]
    async fn verify_ok_invalid_is_invalid_not_error() {
        let recorder = DebuggingRecorder::new();
        let snapshotter = recorder.snapshotter();
        let _guard = metrics::set_default_local_recorder(&recorder);
        let state: FacilitatorState = Arc::new(InvalidVerify);
        let (status, json) = send(
            routes().with_state(state),
            json_request("POST", "/verify", &ts_client_body()),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "200");
        assert_eq!(json["invalidReason"], "insufficient_funds", "scheme");
        assert_counter(&snapshotter, "facilitator_http_verify_total", "invalid", 1);
    }

    #[tokio::test]
    async fn verify_onchain_err_is_error() {
        let recorder = DebuggingRecorder::new();
        let snapshotter = recorder.snapshotter();
        let _guard = metrics::set_default_local_recorder(&recorder);
        let state: FacilitatorState = Arc::new(ErroringFacilitator);
        let (status, _) = send(
            routes().with_state(state),
            json_request("POST", "/verify", &ts_client_body()),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "200");
        assert_counter(&snapshotter, "facilitator_http_verify_total", "error", 1);
    }

    #[tokio::test]
    async fn verify_missing_handler_is_invalid() {
        let recorder = DebuggingRecorder::new();
        let snapshotter = recorder.snapshotter();
        let _guard = metrics::set_default_local_recorder(&recorder);
        let state: FacilitatorState = Arc::new(SchemeRegistry::new());
        let (status, json) = send(
            routes().with_state(state),
            json_request("POST", "/verify", &ts_client_body()),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "200");
        assert_eq!(
            json["invalidReason"], "no_facilitator_for_network",
            "reason"
        );
        assert_counter(&snapshotter, "facilitator_http_verify_total", "invalid", 1);
    }

    #[tokio::test]
    async fn settle_success() {
        let recorder = DebuggingRecorder::new();
        let snapshotter = recorder.snapshotter();
        let _guard = metrics::set_default_local_recorder(&recorder);
        let (status, _) = send(
            stub_app(),
            json_request("POST", "/settle", &ts_client_body()),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "200");
        assert_counter(&snapshotter, "facilitator_http_settle_total", "success", 1);
    }

    #[tokio::test]
    async fn settle_ok_failure_is_failure() {
        let recorder = DebuggingRecorder::new();
        let snapshotter = recorder.snapshotter();
        let _guard = metrics::set_default_local_recorder(&recorder);
        let state: FacilitatorState = Arc::new(FailingSettle);
        let (status, _) = send(
            routes().with_state(state),
            json_request("POST", "/settle", &ts_client_body()),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "200");
        assert_counter(&snapshotter, "facilitator_http_settle_total", "failure", 1);
    }

    #[tokio::test]
    async fn settle_envelope_is_failure() {
        let recorder = DebuggingRecorder::new();
        let snapshotter = recorder.snapshotter();
        let _guard = metrics::set_default_local_recorder(&recorder);
        let (status, _) = send(stub_app(), json_request("POST", "/settle", &json!({}))).await;
        assert_eq!(status, StatusCode::OK, "200");
        assert_counter(&snapshotter, "facilitator_http_settle_total", "failure", 1);
    }

    #[tokio::test]
    async fn settle_onchain_err_is_error() {
        let recorder = DebuggingRecorder::new();
        let snapshotter = recorder.snapshotter();
        let _guard = metrics::set_default_local_recorder(&recorder);
        let state: FacilitatorState = Arc::new(ErroringFacilitator);
        let (status, _) = send(
            routes().with_state(state),
            json_request("POST", "/settle", &ts_client_body()),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "200");
        assert_counter(&snapshotter, "facilitator_http_settle_total", "error", 1);
    }

    #[tokio::test]
    async fn settle_json_rejection_is_error() {
        let recorder = DebuggingRecorder::new();
        let snapshotter = recorder.snapshotter();
        let _guard = metrics::set_default_local_recorder(&recorder);
        let req = Request::builder()
            .method("POST")
            .uri("/settle")
            .header("content-type", "application/json")
            .body(Body::from("not-json"))
            .unwrap();
        let (status, _) = send(stub_app(), req).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "400");
        assert_counter(&snapshotter, "facilitator_http_settle_total", "error", 1);
    }

    #[tokio::test]
    async fn settle_missing_handler_is_failure() {
        let recorder = DebuggingRecorder::new();
        let snapshotter = recorder.snapshotter();
        let _guard = metrics::set_default_local_recorder(&recorder);
        let state: FacilitatorState = Arc::new(SchemeRegistry::new());
        let (status, json) = send(
            routes().with_state(state),
            json_request("POST", "/settle", &ts_client_body()),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "200");
        assert_eq!(json["errorReason"], "no_facilitator_for_network", "reason");
        assert_counter(&snapshotter, "facilitator_http_settle_total", "failure", 1);
    }
}
