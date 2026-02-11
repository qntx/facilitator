//! Core facilitator implementation for x402 payments.
//!
//! [`FacilitatorLocal`] routes payment verification and settlement requests
//! to the appropriate scheme handler via a [`SchemeRegistry`].
//!
//! Payment-level errors (unsupported scheme, invalid payload, etc.) are returned
//! as `Ok(VerifyResponse::Invalid)` / `Ok(SettleResponse::Error)` with HTTP 200,
//! matching the official CDP facilitator behavior.
//! Only operational errors (RPC failures, etc.) propagate as `Err(FacilitatorError)`.

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;

use r402::facilitator::{Facilitator, FacilitatorError};
use r402::proto;
use r402::proto::AsPaymentProblem;
use r402::scheme::SchemeRegistry;

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
            // Payment-level routing failure → 200 + Invalid (matches CDP behavior)
            let handler = match request
                .scheme_slug()
                .and_then(|slug| self.handlers.by_slug(&slug))
            {
                Some(h) => h,
                None => {
                    return Ok(proto::VerifyResponse::invalid(
                        None,
                        "unsupported_scheme".into(),
                    ));
                }
            };
            handler.verify(request).await
        })
    }

    fn settle(
        &self,
        request: proto::SettleRequest,
    ) -> Pin<Box<dyn Future<Output = Result<proto::SettleResponse, FacilitatorError>> + Send + '_>>
    {
        Box::pin(async move {
            // Payment-level routing failure → 200 + Error (matches CDP behavior)
            let handler = match request
                .scheme_slug()
                .and_then(|slug| self.handlers.by_slug(&slug))
            {
                Some(h) => h,
                None => {
                    return Ok(proto::SettleResponse::Error {
                        reason: "unsupported_scheme".into(),
                        message: Some("No handler registered for this payment scheme".into()),
                        payer: None,
                        network: String::new(),
                    });
                }
            };
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

/// Converts a [`FacilitatorError`] into a [`proto::VerifyResponse::Invalid`].
///
/// Used by the `/verify` route to return a well-formed error response
/// using the official x402 verify wire format when an operational error occurs.
pub fn error_to_verify_response(error: &FacilitatorError) -> proto::VerifyResponse {
    let problem = error.as_payment_problem();
    let reason = error_reason_string(problem.reason());
    proto::VerifyResponse::invalid_with_message(None, reason, problem.details().to_owned())
}

/// Converts a [`FacilitatorError`] into a [`proto::SettleResponse::Error`].
///
/// Used by the `/settle` route to return a well-formed error response
/// using the official x402 settle wire format when an operational error occurs.
pub fn error_to_settle_response(error: &FacilitatorError) -> proto::SettleResponse {
    let problem = error.as_payment_problem();
    let reason = error_reason_string(problem.reason());
    proto::SettleResponse::Error {
        reason,
        message: Some(problem.details().to_owned()),
        payer: None,
        network: String::new(),
    }
}

/// Serializes an [`ErrorReason`] enum variant to its snake_case string representation.
fn error_reason_string(reason: proto::ErrorReason) -> String {
    serde_json::to_value(reason)
        .ok()
        .and_then(|v| match v {
            serde_json::Value::String(s) => Some(s),
            _ => None,
        })
        .unwrap_or_else(|| "unexpected_error".to_owned())
}
