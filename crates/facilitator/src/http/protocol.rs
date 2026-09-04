//! Spec §7 protocol handlers.

use axum::Json;
use axum::extract::State;
use axum::extract::rejection::JsonRejection;
use axum::http::{HeaderName, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use r402_facilitator::DynFacilitator;
use r402_protocol::error::ErrorReason;
use r402_protocol::payment::{
    Base64Bytes, Extensions, SettleRequest, SettleResponse, SupportedResponse, VerifyRequest,
    VerifyResponse,
};
use r402_protocol::scheme::SchemeSlug;
use tracing::Instrument;

use super::AppState;
use super::classify::{classify_settle_envelope, classify_verify_envelope};
use crate::metrics::{
    SettleMetric, SettleResult, VerifyMetric, VerifyResult, settle_from_error, verify_from_error,
};

/// Facilitator-only sidechannel. Not a buyer CORS header.
const EXTENSION_RESPONSES: &str = "EXTENSION-RESPONSES";

/// `GET /supported` — spec §7.3. Empty map yields `kinds: []`.
pub(super) async fn get_supported(
    State(state): State<AppState>,
) -> Result<Json<SupportedResponse>, StatusCode> {
    match DynFacilitator::supported(state.facilitator.as_ref()).await {
        Ok(supported) => Ok(Json(supported)),
        Err(error) => {
            tracing::error!(?error, "supported aggregation failed");
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// `POST /verify` — spec §7.1.
pub(super) async fn post_verify(
    State(state): State<AppState>,
    body: Result<Json<VerifyRequest>, JsonRejection>,
) -> Response {
    let span = tracing::info_span!(
        "x402.facilitator.verify",
        network = tracing::field::Empty,
        scheme = tracing::field::Empty,
        x402.version = 2,
        http.status_code = tracing::field::Empty,
        result = tracing::field::Empty,
        otel.kind = "server",
    );
    verify_inner(state, body).instrument(span).await
}

/// `POST /settle` — spec §7.2.
pub(super) async fn post_settle(
    State(state): State<AppState>,
    body: Result<Json<SettleRequest>, JsonRejection>,
) -> Response {
    let span = tracing::info_span!(
        "x402.facilitator.settle",
        network = tracing::field::Empty,
        scheme = tracing::field::Empty,
        x402.version = 2,
        http.status_code = tracing::field::Empty,
        result = tracing::field::Empty,
        otel.kind = "server",
    );
    settle_inner(state, body).instrument(span).await
}

async fn verify_inner(
    state: AppState,
    body: Result<Json<VerifyRequest>, JsonRejection>,
) -> Response {
    let mut metric = VerifyMetric::start();
    let Ok(Json(request)) = body else {
        metric.finish(VerifyResult::Error);
        record_outcome(StatusCode::BAD_REQUEST, "error");
        return invalid_request_body();
    };
    record_slug(request.scheme_slug().as_ref());
    if let Some(resp) = classify_verify_envelope(&request) {
        metric.finish(VerifyResult::Invalid);
        record_outcome(StatusCode::OK, "invalid");
        return protocol_json(StatusCode::OK, &resp, resp.extension_responses());
    }
    match DynFacilitator::verify(state.facilitator.as_ref(), request).await {
        Ok(resp) => finish_verify(&mut metric, &resp),
        Err(error) => {
            tracing::warn!(?error, "verification failed");
            let result = verify_from_error(&error);
            metric.finish(result);
            verify_error_response(&error)
        }
    }
}

async fn settle_inner(
    state: AppState,
    body: Result<Json<SettleRequest>, JsonRejection>,
) -> Response {
    let mut metric = SettleMetric::start();
    let Ok(Json(request)) = body else {
        metric.finish(SettleResult::Error);
        record_outcome(StatusCode::BAD_REQUEST, "error");
        return invalid_request_body();
    };
    record_slug(request.scheme_slug().as_ref());
    if let Some(resp) = classify_settle_envelope(&request) {
        metric.finish(SettleResult::Failure);
        record_outcome(StatusCode::OK, "failure");
        return protocol_json(StatusCode::OK, &resp, resp.extension_responses());
    }
    let network = request.network().to_owned();
    match DynFacilitator::settle(state.facilitator.as_ref(), request).await {
        Ok(resp) => finish_settle(&mut metric, &resp),
        Err(error) => {
            tracing::warn!(?error, "settlement failed");
            let result = settle_from_error(&error);
            metric.finish(result);
            settle_error_response(&error, &network)
        }
    }
}

fn finish_verify(metric: &mut VerifyMetric, resp: &VerifyResponse) -> Response {
    let result = VerifyResult::from_response(resp);
    metric.finish(result);
    record_outcome(StatusCode::OK, result.as_str());
    protocol_json(StatusCode::OK, resp, resp.extension_responses())
}

fn finish_settle(metric: &mut SettleMetric, resp: &SettleResponse) -> Response {
    let result = SettleResult::from_response(resp);
    metric.finish(result);
    record_outcome(StatusCode::OK, result.as_str());
    protocol_json(StatusCode::OK, resp, resp.extension_responses())
}

fn verify_error_response(error: &r402_protocol::error::FacilitatorError) -> Response {
    VerifyResponse::from_facilitator_error(error).map_or_else(
        || {
            record_outcome(StatusCode::BAD_GATEWAY, "error");
            json_error(StatusCode::BAD_GATEWAY, "facilitator transport")
        },
        |resp| {
            let status = verify_http_status(&resp);
            record_outcome(status, "invalid");
            protocol_json(status, &resp, resp.extension_responses())
        },
    )
}

fn settle_error_response(
    error: &r402_protocol::error::FacilitatorError,
    network: &str,
) -> Response {
    SettleResponse::from_facilitator_error(error, network, "").map_or_else(
        || {
            record_outcome(StatusCode::BAD_GATEWAY, "error");
            json_error(StatusCode::BAD_GATEWAY, "facilitator transport")
        },
        |resp| {
            record_outcome(StatusCode::OK, "failure");
            protocol_json(StatusCode::OK, &resp, resp.extension_responses())
        },
    )
}

const fn verify_http_status(resp: &VerifyResponse) -> StatusCode {
    match resp {
        VerifyResponse::Invalid {
            reason: Some(ErrorReason::Permit2AllowanceRequired),
            ..
        } => StatusCode::PRECONDITION_FAILED,
        _ => StatusCode::OK,
    }
}

fn protocol_json<T: serde::Serialize>(status: StatusCode, body: &T, side: &Extensions) -> Response {
    let mut response = (status, Json(body)).into_response();
    attach_extension_responses(response.headers_mut(), side);
    response
}

fn attach_extension_responses(headers: &mut axum::http::HeaderMap, side: &Extensions) {
    if side.is_empty() {
        return;
    }
    let Ok(json) = serde_json::to_vec(side) else {
        return;
    };
    let encoded = Base64Bytes::encode(json);
    let Ok(name) = HeaderName::from_bytes(EXTENSION_RESPONSES.as_bytes()) else {
        return;
    };
    let Ok(value) = HeaderValue::try_from(encoded.as_ref()) else {
        return;
    };
    headers.insert(name, value);
}

fn invalid_request_body() -> Response {
    json_error(StatusCode::BAD_REQUEST, "invalid request body")
}

fn json_error(status: StatusCode, message: &'static str) -> Response {
    (status, Json(serde_json::json!({ "error": message }))).into_response()
}

fn record_slug(slug: Option<&SchemeSlug>) {
    let Some(slug) = slug else {
        return;
    };
    let span = tracing::Span::current();
    span.record("network", tracing::field::display(&slug.chain_id));
    span.record("scheme", slug.name.as_str());
}

fn record_outcome(status: StatusCode, result: &'static str) {
    let span = tracing::Span::current();
    span.record("http.status_code", status.as_u16());
    span.record("result", result);
}
