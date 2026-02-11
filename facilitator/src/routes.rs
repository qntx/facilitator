//! HTTP route handlers for the x402 facilitator.
//!
//! Protocol endpoints (`/verify`, `/settle`, `/supported`, `/health`).
//! All payloads use JSON, compatible with official x402 client SDKs.

use std::sync::Arc;

use axum::extract::State;
use axum::extract::rejection::JsonRejection;
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router, response::IntoResponse};
use r402::facilitator::Facilitator;
use r402::proto;
use serde_json::json;
#[cfg(feature = "telemetry")]
use tracing::instrument;

use crate::facilitator::FacilitatorLocal;

/// Creates the Axum router with all x402 facilitator endpoints.
pub fn routes() -> Router<Arc<FacilitatorLocal>> {
    Router::new()
        .route("/", get(get_root))
        .route("/verify", post(post_verify))
        .route("/settle", post(post_settle))
        .route("/health", get(get_health))
        .route("/supported", get(get_supported))
}

/// `GET /` — simple greeting.
#[cfg_attr(feature = "telemetry", instrument(skip_all))]
async fn get_root() -> impl IntoResponse {
    (
        StatusCode::OK,
        concat!("Hello from ", env!("CARGO_PKG_NAME"), "!"),
    )
}

/// `GET /health` — lightweight liveness check.
#[cfg_attr(feature = "telemetry", instrument(skip_all))]
async fn get_health() -> impl IntoResponse {
    (StatusCode::OK, Json(json!({ "status": "ok" })))
}

/// `GET /supported` — lists supported payment schemes and networks.
#[cfg_attr(feature = "telemetry", instrument(skip_all))]
async fn get_supported(State(facilitator): State<Arc<FacilitatorLocal>>) -> impl IntoResponse {
    match facilitator.supported().await {
        Ok(supported) => (StatusCode::OK, Json(json!(supported))).into_response(),
        Err(error) => {
            #[cfg(feature = "telemetry")]
            tracing::error!(error = ?error, "Failed to query supported schemes");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": error.to_string() })),
            )
                .into_response()
        }
    }
}

/// `POST /verify` — verify a proposed x402 payment.
#[cfg_attr(feature = "telemetry", instrument(skip_all))]
async fn post_verify(
    State(facilitator): State<Arc<FacilitatorLocal>>,
    body: Result<Json<proto::VerifyRequest>, JsonRejection>,
) -> impl IntoResponse {
    let Json(body) = match body {
        Ok(b) => b,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "Invalid request body" })),
            )
                .into_response();
        }
    };
    match facilitator.verify(body).await {
        Ok(response) => (StatusCode::OK, Json(json!(response))).into_response(),
        Err(error) => {
            #[cfg(feature = "telemetry")]
            tracing::warn!(error = ?error, "Verification failed");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": error.to_string() })),
            )
                .into_response()
        }
    }
}

/// `POST /settle` — settle a verified x402 payment on-chain.
#[cfg_attr(feature = "telemetry", instrument(skip_all))]
async fn post_settle(
    State(facilitator): State<Arc<FacilitatorLocal>>,
    body: Result<Json<proto::SettleRequest>, JsonRejection>,
) -> impl IntoResponse {
    let Json(body) = match body {
        Ok(b) => b,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "Invalid request body" })),
            )
                .into_response();
        }
    };
    match facilitator.settle(body).await {
        Ok(response) => (StatusCode::OK, Json(json!(response))).into_response(),
        Err(error) => {
            #[cfg(feature = "telemetry")]
            tracing::warn!(error = ?error, "Settlement failed");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": error.to_string() })),
            )
                .into_response()
        }
    }
}
