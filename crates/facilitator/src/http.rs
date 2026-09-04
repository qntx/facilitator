//! Axum router for facilitator protocol routes.

use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use r402_facilitator::DynFacilitator;
use r402_protocol::payment::SupportedResponse;

/// Shared HTTP state.
#[derive(Clone)]
pub struct AppState {
    /// In-process scheme handlers.
    facilitator: Arc<dyn DynFacilitator>,
}

impl std::fmt::Debug for AppState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AppState").finish_non_exhaustive()
    }
}

impl AppState {
    /// Wrap a facilitator for the router.
    #[must_use]
    pub fn new(facilitator: Arc<dyn DynFacilitator>) -> Self {
        Self { facilitator }
    }
}

/// Protocol router.
pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/supported", get(get_supported))
        .with_state(state)
}

/// `GET /supported` — spec §7.3. Empty map yields `kinds: []`.
async fn get_supported(
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
