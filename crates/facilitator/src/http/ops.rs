//! Liveness and readiness. Unauthenticated for kubelet.

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;

use super::AppState;

/// `GET /healthz` — process liveness. No RPC.
pub(super) async fn get_healthz() -> Json<serde_json::Value> {
    Json(serde_json::json!({ "status": "ok" }))
}

/// `GET /readyz` — 200 when the map is nonempty; 503 otherwise.
pub(super) async fn get_readyz(
    State(state): State<AppState>,
) -> (StatusCode, Json<serde_json::Value>) {
    if state.ready {
        (StatusCode::OK, Json(serde_json::json!({ "status": "ok" })))
    } else {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({ "status": "not ready" })),
        )
    }
}
