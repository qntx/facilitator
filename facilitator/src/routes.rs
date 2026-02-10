//! HTTP route handlers for the x402 facilitator.
//!
//! Protocol-critical endpoints (`/verify`, `/settle`) and discovery
//! endpoints (`/supported`, `/health`). All payloads use JSON and are
//! compatible with official x402 client SDKs.

use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router, response::IntoResponse};
use r402::facilitator::Facilitator;
use r402::proto;
use serde_json::json;

#[cfg(feature = "telemetry")]
use tracing::instrument;

/// Creates the Axum router with all x402 facilitator endpoints.
pub fn routes<A>() -> Router<A>
where
    A: Facilitator + Clone + Send + Sync + 'static,
    A::Error: IntoResponse,
{
    Router::new()
        .route("/", get(get_root))
        .route("/verify", get(get_verify_info))
        .route("/verify", post(post_verify::<A>))
        .route("/settle", get(get_settle_info))
        .route("/settle", post(post_settle::<A>))
        .route("/health", get(get_health::<A>))
        .route("/supported", get(get_supported::<A>))
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
pub async fn get_supported<A>(State(facilitator): State<A>) -> impl IntoResponse
where
    A: Facilitator,
    A::Error: IntoResponse,
{
    match facilitator.supported().await {
        Ok(supported) => (StatusCode::OK, Json(json!(supported))).into_response(),
        Err(error) => error.into_response(),
    }
}

/// `GET /health` — health check (delegates to `/supported`).
#[cfg_attr(feature = "telemetry", instrument(skip_all))]
pub async fn get_health<A>(State(facilitator): State<A>) -> impl IntoResponse
where
    A: Facilitator,
    A::Error: IntoResponse,
{
    get_supported(State(facilitator)).await
}

/// `POST /verify` — verify a proposed x402 payment.
#[cfg_attr(feature = "telemetry", instrument(skip_all))]
pub async fn post_verify<A>(
    State(facilitator): State<A>,
    Json(body): Json<proto::VerifyRequest>,
) -> impl IntoResponse
where
    A: Facilitator,
    A::Error: IntoResponse,
{
    match facilitator.verify(&body).await {
        Ok(valid_response) => (StatusCode::OK, Json(valid_response)).into_response(),
        Err(error) => {
            #[cfg(feature = "telemetry")]
            tracing::warn!(
                error = ?error,
                body = %serde_json::to_string(&body).unwrap_or_else(|_| "<can-not-serialize>".to_string()),
                "Verification failed"
            );
            error.into_response()
        }
    }
}

/// `POST /settle` — settle a verified x402 payment on-chain.
#[cfg_attr(feature = "telemetry", instrument(skip_all))]
pub async fn post_settle<A>(
    State(facilitator): State<A>,
    Json(body): Json<proto::SettleRequest>,
) -> impl IntoResponse
where
    A: Facilitator,
    A::Error: IntoResponse,
{
    match facilitator.settle(&body).await {
        Ok(valid_response) => (StatusCode::OK, Json(valid_response)).into_response(),
        Err(error) => {
            #[cfg(feature = "telemetry")]
            tracing::warn!(
                error = ?error,
                body = %serde_json::to_string(&body).unwrap_or_else(|_| "<can-not-serialize>".to_string()),
                "Settlement failed"
            );
            error.into_response()
        }
    }
}
