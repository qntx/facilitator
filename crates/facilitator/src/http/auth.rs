//! Optional shared bearer on protocol routes.

use axum::Json;
use axum::extract::{Request, State};
use axum::http::{StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

use super::AppState;

/// Require `Authorization: Bearer` when `[http.auth]` is configured.
pub(super) async fn require_bearer(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Response {
    let Some(expected) = state.bearer.as_deref() else {
        return next.run(request).await;
    };
    if bearer_matches(request.headers().get(header::AUTHORIZATION), expected) {
        return next.run(request).await;
    }
    (
        StatusCode::UNAUTHORIZED,
        Json(serde_json::json!({ "error": "unauthorized" })),
    )
        .into_response()
}

fn bearer_matches(header: Option<&axum::http::HeaderValue>, expected: &str) -> bool {
    let Some(value) = header.and_then(|v| v.to_str().ok()) else {
        return false;
    };
    value
        .strip_prefix("Bearer ")
        .is_some_and(|token| token == expected)
}
