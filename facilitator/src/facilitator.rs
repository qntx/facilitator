//! Core facilitator implementation for x402 payments.
//!
//! [`FacilitatorLocal`] routes payment verification and settlement requests
//! to the appropriate scheme handler via a [`SchemeRegistry`].

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;

use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use r402::facilitator::{Facilitator, FacilitatorError};
use r402::proto;
use r402::proto::{AsPaymentProblem, ErrorReason, PaymentVerificationError};
use r402::scheme::SchemeRegistry;
use serde::{Deserialize, Serialize};

/// Local [`Facilitator`] that delegates to scheme handlers in a [`SchemeRegistry`].
#[allow(missing_debug_implementations)]
pub struct FacilitatorLocal {
    handlers: SchemeRegistry,
}

impl FacilitatorLocal {
    /// Creates a new [`FacilitatorLocal`] with the given handler registry.
    pub const fn new(handlers: SchemeRegistry) -> Self {
        Self { handlers }
    }
}

impl Facilitator for FacilitatorLocal {
    fn verify(
        &self,
        request: proto::VerifyRequest,
    ) -> Pin<Box<dyn Future<Output = Result<proto::VerifyResponse, FacilitatorError>> + Send + '_>>
    {
        Box::pin(async move {
            let handler = request
                .scheme_slug()
                .and_then(|slug| self.handlers.by_slug(&slug))
                .ok_or_else(|| {
                    FacilitatorError::PaymentVerification(
                        PaymentVerificationError::UnsupportedScheme,
                    )
                })?;
            handler.verify(request).await
        })
    }

    fn settle(
        &self,
        request: proto::SettleRequest,
    ) -> Pin<Box<dyn Future<Output = Result<proto::SettleResponse, FacilitatorError>> + Send + '_>>
    {
        Box::pin(async move {
            let handler = request
                .scheme_slug()
                .and_then(|slug| self.handlers.by_slug(&slug))
                .ok_or_else(|| {
                    FacilitatorError::PaymentVerification(
                        PaymentVerificationError::UnsupportedScheme,
                    )
                })?;
            handler.settle(request).await
        })
    }

    fn supported(
        &self,
    ) -> Pin<Box<dyn Future<Output = Result<proto::SupportedResponse, FacilitatorError>> + Send + '_>>
    {
        Box::pin(async move {
            let mut kinds = vec![];
            let mut signers = HashMap::new();
            for provider in self.handlers.values() {
                let supported = provider.supported().await.ok();
                if let Some(mut supported) = supported {
                    kinds.append(&mut supported.kinds);
                    for (chain_id, signer_addresses) in supported.signers {
                        signers.entry(chain_id).or_insert(signer_addresses);
                    }
                }
            }
            Ok(proto::SupportedResponse {
                kinds,
                extensions: Vec::new(),
                signers,
            })
        })
    }
}

/// Errors from local facilitator operations.
///
/// Wraps [`FacilitatorError`] to provide HTTP response conversion.
#[derive(Debug, thiserror::Error)]
#[error(transparent)]
pub struct FacilitatorLocalError(pub FacilitatorError);

impl From<FacilitatorError> for FacilitatorLocalError {
    fn from(err: FacilitatorError) -> Self {
        Self(err)
    }
}

impl IntoResponse for FacilitatorLocalError {
    fn into_response(self) -> Response {
        #[derive(Serialize, Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct VerificationErrorResponse {
            is_valid: bool,
            invalid_reason: ErrorReason,
            invalid_reason_details: String,
            payer: String,
        }

        let problem = self.0.as_payment_problem();
        let status = match &self.0 {
            FacilitatorError::PaymentVerification(_) => StatusCode::BAD_REQUEST,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        };
        let body = VerificationErrorResponse {
            is_valid: false,
            invalid_reason: problem.reason(),
            invalid_reason_details: problem.details().to_owned(),
            payer: String::new(),
        };
        (status, Json(body)).into_response()
    }
}
