//! Core facilitator implementation for x402 payments.
//!
//! [`FacilitatorLocal`] routes payment verification and settlement requests
//! to the appropriate scheme handler via a [`SchemeRegistry`].

use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use r402::facilitator::Facilitator;
use r402::proto;
use r402::proto::{AsPaymentProblem, ErrorReason, PaymentVerificationError};
use r402::scheme::{SchemeRegistry, X402SchemeFacilitatorError};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Local [`Facilitator`] that delegates to scheme handlers in a [`SchemeRegistry`].
#[allow(missing_debug_implementations)]
pub struct FacilitatorLocal<A> {
    handlers: A,
}

impl<A> FacilitatorLocal<A> {
    /// Creates a new [`FacilitatorLocal`] with the given handler registry.
    pub const fn new(handlers: A) -> Self {
        Self { handlers }
    }
}

impl Facilitator for FacilitatorLocal<SchemeRegistry> {
    type Error = FacilitatorLocalError;

    async fn verify(
        &self,
        request: &proto::VerifyRequest,
    ) -> Result<proto::VerifyResponse, Self::Error> {
        let handler = request
            .scheme_handler_slug()
            .and_then(|slug| self.handlers.by_slug(&slug))
            .ok_or_else(|| {
                FacilitatorLocalError::Verification(
                    PaymentVerificationError::UnsupportedScheme.into(),
                )
            })?;
        let response = handler
            .verify(request)
            .await
            .map_err(FacilitatorLocalError::Verification)?;
        Ok(response)
    }

    async fn settle(
        &self,
        request: &proto::SettleRequest,
    ) -> Result<proto::SettleResponse, Self::Error> {
        let handler = request
            .scheme_handler_slug()
            .and_then(|slug| self.handlers.by_slug(&slug))
            .ok_or_else(|| {
                FacilitatorLocalError::Verification(
                    PaymentVerificationError::UnsupportedScheme.into(),
                )
            })?;
        let response = handler
            .settle(request)
            .await
            .map_err(FacilitatorLocalError::Settlement)?;
        Ok(response)
    }

    async fn supported(&self) -> Result<proto::SupportedResponse, Self::Error> {
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
    }
}

/// Errors from local facilitator operations.
#[derive(Debug, thiserror::Error)]
pub enum FacilitatorLocalError {
    /// Payment verification failed.
    #[error(transparent)]
    Verification(X402SchemeFacilitatorError),
    /// Payment settlement failed.
    #[error(transparent)]
    Settlement(X402SchemeFacilitatorError),
}

impl IntoResponse for FacilitatorLocalError {
    fn into_response(self) -> Response {
        #[derive(Serialize, Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct VerificationErrorResponse<'a> {
            is_valid: bool,
            invalid_reason: ErrorReason,
            invalid_reason_details: &'a str,
            payer: &'a str,
        }

        #[derive(Serialize, Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct SettlementErrorResponse<'a> {
            success: bool,
            network: &'a str,
            transaction: &'a str,
            error_reason: ErrorReason,
            error_reason_details: &'a str,
            payer: &'a str,
        }

        match self {
            Self::Verification(scheme_handler_error) => {
                let problem = scheme_handler_error.as_payment_problem();
                let body = VerificationErrorResponse {
                    is_valid: false,
                    invalid_reason: problem.reason(),
                    invalid_reason_details: problem.details(),
                    payer: "",
                };
                let status = match scheme_handler_error {
                    X402SchemeFacilitatorError::PaymentVerification(_) => StatusCode::BAD_REQUEST,
                    X402SchemeFacilitatorError::OnchainFailure(_) => {
                        StatusCode::INTERNAL_SERVER_ERROR
                    }
                };
                (status, Json(body)).into_response()
            }
            Self::Settlement(scheme_handler_error) => {
                let problem = scheme_handler_error.as_payment_problem();
                let body = SettlementErrorResponse {
                    success: false,
                    network: "",
                    transaction: "",
                    error_reason: problem.reason(),
                    error_reason_details: problem.details(),
                    payer: "",
                };
                let status = match scheme_handler_error {
                    X402SchemeFacilitatorError::PaymentVerification(_) => StatusCode::BAD_REQUEST,
                    X402SchemeFacilitatorError::OnchainFailure(_) => {
                        StatusCode::INTERNAL_SERVER_ERROR
                    }
                };
                (status, Json(body)).into_response()
            }
        }
    }
}
