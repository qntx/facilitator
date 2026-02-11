//! HTTP route handlers for the x402 facilitator.
//!
//! Protocol-critical endpoints (`/verify`, `/settle`) and discovery
//! endpoints (`/supported`, `/health`). All payloads use JSON and are
//! compatible with official x402 client SDKs.

use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router, response::IntoResponse};
use r402::proto;
use serde_json::json;
#[cfg(feature = "telemetry")]
use tracing::instrument;

use crate::facilitator::{FacilitatorLocal, error_to_settle_response, error_to_verify_response};

/// Creates the Axum router with all x402 facilitator endpoints.
pub fn routes() -> Router<Arc<FacilitatorLocal>> {
    Router::new()
        .route("/", get(get_root))
        .route("/verify", get(get_verify_info))
        .route("/verify", post(post_verify))
        .route("/settle", get(get_settle_info))
        .route("/settle", post(post_settle))
        .route("/health", get(get_health))
        .route("/supported", get(get_supported))
}

/// `GET /` — simple greeting.
#[cfg_attr(feature = "telemetry", instrument(skip_all))]
pub async fn get_root() -> impl IntoResponse {
    let pkg_name = env!("CARGO_PKG_NAME");
    (StatusCode::OK, format!("Hello from {pkg_name}!"))
}

/// `GET /verify` — endpoint metadata.
#[cfg_attr(feature = "telemetry", instrument(skip_all))]
pub async fn get_verify_info() -> impl IntoResponse {
    Json(json!({
        "endpoint": "/verify",
        "description": "POST to verify x402 payments",
        "body": {
            "paymentPayload": "PaymentPayload",
            "paymentRequirements": "PaymentRequirements",
        }
    }))
}

/// `GET /settle` — endpoint metadata.
#[cfg_attr(feature = "telemetry", instrument(skip_all))]
pub async fn get_settle_info() -> impl IntoResponse {
    Json(json!({
        "endpoint": "/settle",
        "description": "POST to settle x402 payments",
        "body": {
            "paymentPayload": "PaymentPayload",
            "paymentRequirements": "PaymentRequirements",
        }
    }))
}

/// `GET /supported` — lists supported payment schemes and networks.
#[cfg_attr(feature = "telemetry", instrument(skip_all))]
pub async fn get_supported(State(facilitator): State<Arc<FacilitatorLocal>>) -> impl IntoResponse {
    use r402::facilitator::Facilitator;
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

/// `GET /health` — health check (delegates to `/supported`).
#[cfg_attr(feature = "telemetry", instrument(skip_all))]
pub async fn get_health(State(facilitator): State<Arc<FacilitatorLocal>>) -> impl IntoResponse {
    get_supported(State(facilitator)).await
}

/// `POST /verify` — verify a proposed x402 payment.
#[cfg_attr(feature = "telemetry", instrument(skip_all))]
pub async fn post_verify(
    State(facilitator): State<Arc<FacilitatorLocal>>,
    Json(body): Json<proto::VerifyRequest>,
) -> impl IntoResponse {
    use r402::facilitator::Facilitator;
    match facilitator.verify(body).await {
        Ok(response) => (StatusCode::OK, Json(response)).into_response(),
        Err(error) => {
            #[cfg(feature = "telemetry")]
            tracing::warn!(error = ?error, "Verification failed");
            let response = error_to_verify_response(&error);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(response)).into_response()
        }
    }
}

/// `POST /settle` — settle a verified x402 payment on-chain.
#[cfg_attr(feature = "telemetry", instrument(skip_all))]
pub async fn post_settle(
    State(facilitator): State<Arc<FacilitatorLocal>>,
    Json(body): Json<proto::SettleRequest>,
) -> impl IntoResponse {
    use r402::facilitator::Facilitator;
    match facilitator.settle(body).await {
        Ok(response) => (StatusCode::OK, Json(response)).into_response(),
        Err(error) => {
            #[cfg(feature = "telemetry")]
            tracing::warn!(error = ?error, "Settlement failed");
            let response = error_to_settle_response(&error);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(response)).into_response()
        }
    }
}
