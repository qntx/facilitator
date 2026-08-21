//! HTTP route handlers for the x402 facilitator.
//!
//! Protocol endpoints (`POST /verify`, `POST /settle`, `GET /supported`) plus
//! process `GET /health`. Protocol outcomes are HTTP 200 with structured
//! bodies. HTTP 400 is reserved for Axum `JsonRejection`.

use std::sync::Arc;
use std::time::Duration;

use axum::extract::State;
use axum::extract::rejection::JsonRejection;
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router, response::IntoResponse};
use r402_core::error::ErrorReason;
use r402_core::facilitator::{DynFacilitator, Facilitator};
use r402_core::wire::{
    Extensions, SettleRequest, SettleResponse, SupportedResponse, VerifyRequest, VerifyResponse,
};
use tower_http::timeout::TimeoutLayer;
use tracing::instrument;

use crate::metrics::{
    SettleMetric, SettleResult, VerifyMetric, VerifyResult, settle_from_error, verify_from_error,
};

/// Shared facilitator used by Axum handlers.
pub type FacilitatorState = Arc<dyn DynFacilitator>;

/// Creates the Axum router with x402 protocol endpoints and process health.
pub fn routes() -> Router<FacilitatorState> {
    let protocol = Router::new()
        .route("/verify", post(post_verify))
        .route("/settle", post(post_settle))
        .route("/supported", get(get_supported))
        .layer(TimeoutLayer::with_status_code(
            StatusCode::GATEWAY_TIMEOUT,
            Duration::from_secs(30),
        ));
    let health =
        Router::new()
            .route("/health", get(get_health))
            .layer(TimeoutLayer::with_status_code(
                StatusCode::GATEWAY_TIMEOUT,
                Duration::from_secs(5),
            ));
    Router::new().merge(protocol).merge(health)
}

/// `GET /health` — lightweight liveness check (process, not protocol).
#[instrument(skip_all)]
async fn get_health() -> impl IntoResponse {
    (StatusCode::OK, Json(serde_json::json!({ "status": "ok" })))
}

/// `GET /supported` — always HTTP 200 with registry output.
#[instrument(skip_all)]
async fn get_supported(State(facilitator): State<FacilitatorState>) -> impl IntoResponse {
    let response = match Facilitator::supported(&facilitator).await {
        Ok(supported) => supported,
        Err(error) => {
            tracing::error!(error = ?error, "supported aggregation failed");
            SupportedResponse::new()
        }
    };
    (StatusCode::OK, Json(response))
}

/// `POST /verify` — verify a proposed x402 payment.
#[instrument(skip_all)]
async fn post_verify(
    State(facilitator): State<FacilitatorState>,
    body: Result<Json<VerifyRequest>, JsonRejection>,
) -> impl IntoResponse {
    let mut metric = VerifyMetric::start();
    let Ok(Json(request)) = body else {
        metric.finish(VerifyResult::Error);
        return invalid_request_body();
    };
    if let Some(resp) = classify_verify_envelope(&request) {
        metric.finish(VerifyResult::Invalid);
        return (StatusCode::OK, Json(resp)).into_response();
    }
    let response = match Facilitator::verify(&facilitator, request).await {
        Ok(resp) => {
            metric.finish(VerifyResult::from_response(&resp));
            resp
        }
        Err(ref error) => {
            tracing::warn!(?error, "verification failed");
            metric.finish(verify_from_error(error));
            VerifyResponse::from_facilitator_error(error)
        }
    };
    (StatusCode::OK, Json(response)).into_response()
}

/// `POST /settle` — settle a verified x402 payment on-chain.
#[instrument(skip_all)]
async fn post_settle(
    State(facilitator): State<FacilitatorState>,
    body: Result<Json<SettleRequest>, JsonRejection>,
) -> impl IntoResponse {
    let mut metric = SettleMetric::start();
    let Ok(Json(request)) = body else {
        metric.finish(SettleResult::Error);
        return invalid_request_body();
    };
    if let Some(resp) = classify_settle_envelope(&request) {
        metric.finish(SettleResult::Failure);
        return (StatusCode::OK, Json(resp)).into_response();
    }
    let network = request.network().to_owned();
    let response = match Facilitator::settle(&facilitator, request).await {
        Ok(resp) => {
            metric.finish(SettleResult::from_response(&resp));
            resp
        }
        Err(ref error) => {
            tracing::warn!(?error, "settlement failed");
            metric.finish(settle_from_error(error));
            SettleResponse::from_facilitator_error(error, network)
        }
    };
    (StatusCode::OK, Json(response)).into_response()
}

/// 400 is only for unparseable bodies (`JsonRejection`).
fn invalid_request_body() -> axum::response::Response {
    (
        StatusCode::BAD_REQUEST,
        Json(serde_json::json!({ "error": "invalid request body" })),
    )
        .into_response()
}

/// Inspect JSON before `Facilitator::verify`. `None` means the envelope is
/// well-formed and the registry/handler should run.
fn classify_verify_envelope(request: &VerifyRequest) -> Option<VerifyResponse> {
    if request.scheme_slug().is_some() {
        return None;
    }
    Some(VerifyResponse::invalid(
        None,
        envelope_reason(&request_json(request)),
    ))
}

/// Inspect JSON before `Facilitator::settle`.
fn classify_settle_envelope(request: &SettleRequest) -> Option<SettleResponse> {
    if request.scheme_slug().is_some() {
        return None;
    }
    Some(SettleResponse::Failure {
        reason: envelope_reason(&request_json(request)),
        message: None,
        payer: None,
        network: request.network().into(),
        extensions: Extensions::new(),
    })
}

/// Serialise a request for envelope inspection; `Null` if that fails.
fn request_json<T: serde::Serialize>(request: &T) -> serde_json::Value {
    serde_json::to_value(request).unwrap_or(serde_json::Value::Null)
}

/// Version `2` with a bad `accepted` is `invalid_payload`; anything else is `invalid_x402_version`.
fn envelope_reason(json: &serde_json::Value) -> ErrorReason {
    if json.get("x402Version").and_then(serde_json::Value::as_u64) == Some(2) {
        ErrorReason::InvalidPayload
    } else {
        ErrorReason::InvalidX402Version
    }
}
