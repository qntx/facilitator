//! Optional shared bearer on protocol routes.

use axum::Json;
use axum::extract::{Request, State};
use axum::http::{StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use subtle::ConstantTimeEq;

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
    let value = value.trim();
    let Some((scheme, token)) = value.split_once(' ') else {
        return false;
    };
    if !scheme.eq_ignore_ascii_case("bearer") {
        return false;
    }
    token_eq(token.trim(), expected)
}

fn token_eq(provided: &str, expected: &str) -> bool {
    bool::from(provided.as_bytes().ct_eq(expected.as_bytes()))
}

#[cfg(test)]
mod tests {
    use axum::http::HeaderValue;

    use super::*;

    #[test]
    fn scheme_is_case_insensitive() {
        let lower = HeaderValue::from_static("bearer secret");
        assert!(
            bearer_matches(Some(&lower), "secret"),
            "RFC 9110 scheme is case-insensitive"
        );
        let upper = HeaderValue::from_static("BEARER secret");
        assert!(bearer_matches(Some(&upper), "secret"), "uppercase scheme");
    }

    #[test]
    fn token_whitespace_is_trimmed() {
        let header = HeaderValue::from_static("Bearer  secret  ");
        assert!(
            bearer_matches(Some(&header), "secret"),
            "trim around token like resolve_token"
        );
    }

    #[test]
    fn wrong_token_is_rejected() {
        let header = HeaderValue::from_static("Bearer other");
        assert!(!bearer_matches(Some(&header), "secret"), "mismatch");
    }
}
