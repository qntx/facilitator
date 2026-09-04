//! Axum router. PR 1 exposes `GET /supported` only.

use std::sync::Arc;

use axum::extract::State;
use axum::routing::get;
use axum::{Json, Router};
use r402_facilitator::Facilitator;
use r402_protocol::payment::SupportedResponse;

use crate::compose::FacilitatorMap;

/// Shared HTTP state.
#[derive(Clone, Debug)]
pub struct AppState {
    /// Scheme map (may be empty in this PR).
    facilitator: Arc<FacilitatorMap>,
}

impl AppState {
    /// Wrap a map for the router.
    #[must_use]
    pub const fn new(facilitator: Arc<FacilitatorMap>) -> Self {
        Self { facilitator }
    }
}

/// Protocol router. Timeouts, auth, and ops routes are later PRs.
pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/supported", get(get_supported))
        .with_state(state)
}

/// `GET /supported` — spec §7.3. Empty map yields `kinds: []`.
async fn get_supported(State(state): State<AppState>) -> Json<SupportedResponse> {
    let body = match Facilitator::supported(state.facilitator.as_ref()).await {
        Ok(supported) => supported,
        Err(error) => {
            tracing::error!(?error, "supported aggregation failed");
            SupportedResponse::new()
        }
    };
    Json(body)
}
