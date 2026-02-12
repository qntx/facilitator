//! HTTP route handlers for the x402 facilitator.
//!
//! Protocol endpoints (`/verify`, `/settle`, `/supported`, `/health`).
//! All payloads use JSON, compatible with official x402 client SDKs.
//!
//! Error handling follows the x402 wire protocol:
//! - **Verification failures** → HTTP 200 + `VerifyResponse::Invalid`
//! - **Settlement failures** → HTTP 200 + `SettleResponse::Error`
//! - **Infrastructure errors** (only `/supported`) → HTTP 500

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

/// Type alias for the shared facilitator state used by Axum route handlers.
pub type FacilitatorState = Arc<dyn Facilitator>;

/// Creates the Axum router with all x402 facilitator endpoints.
pub fn routes() -> Router<FacilitatorState> {
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
async fn get_supported(State(facilitator): State<FacilitatorState>) -> impl IntoResponse {
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
///
/// All errors are converted to `VerifyResponse::Invalid` (HTTP 200) to preserve
/// structured reason codes on the wire.
#[cfg_attr(feature = "telemetry", instrument(skip_all))]
async fn post_verify(
    State(facilitator): State<FacilitatorState>,
    body: Result<Json<proto::VerifyRequest>, JsonRejection>,
) -> impl IntoResponse {
    let Ok(Json(request)) = body else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "invalid request body" })),
        )
            .into_response();
    };
    let response = match facilitator.verify(request).await {
        Ok(resp) => resp,
        Err(ref error) => {
            #[cfg(feature = "telemetry")]
            tracing::warn!(?error, "verification failed");
            proto::VerifyResponse::from_facilitator_error(error)
        }
    };
    (StatusCode::OK, Json(json!(response))).into_response()
}

/// `POST /settle` — settle a verified x402 payment on-chain.
///
/// All errors are converted to `SettleResponse::Error` (HTTP 200) to preserve
/// structured reason codes on the wire.
#[cfg_attr(feature = "telemetry", instrument(skip_all))]
async fn post_settle(
    State(facilitator): State<FacilitatorState>,
    body: Result<Json<proto::SettleRequest>, JsonRejection>,
) -> impl IntoResponse {
    let Ok(Json(request)) = body else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "invalid request body" })),
        )
            .into_response();
    };
    let network = request.network().to_owned();
    let response = match facilitator.settle(request).await {
        Ok(resp) => resp,
        Err(ref error) => {
            #[cfg(feature = "telemetry")]
            tracing::warn!(?error, "settlement failed");
            proto::SettleResponse::from_facilitator_error(error, network)
        }
    };
    (StatusCode::OK, Json(json!(response))).into_response()
}
