//! Core facilitator implementation for x402 payments.
//!
//! [`FacilitatorLocal`] routes payment verification and settlement requests
//! to the appropriate scheme handler via a [`SchemeRegistry`].
//!
//! All errors (including unsupported scheme) propagate as `Err(FacilitatorError)`
//! and are converted to HTTP 500 + `{"error": "..."}` at the route layer,
//! matching the official x402 Go reference implementation.

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;

use r402::facilitator::{Facilitator, FacilitatorError};
use r402::proto;
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
            let Some(handler) = request
                .scheme_slug()
                .and_then(|slug| self.handlers.by_slug(&slug))
            else {
                return Err(FacilitatorError::Aborted {
                    reason: "no_facilitator_for_network".into(),
                    message: "no handler registered for this payment scheme".into(),
                });
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
            let Some(handler) = request
                .scheme_slug()
                .and_then(|slug| self.handlers.by_slug(&slug))
            else {
                return Err(FacilitatorError::Aborted {
                    reason: "no_facilitator_for_network".into(),
                    message: "no handler registered for this payment scheme".into(),
                });
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
