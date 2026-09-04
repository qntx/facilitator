//! Envelope classifier: missing `scheme_slug` is protocol JSON, not 404.

use r402_protocol::error::ErrorReason;
use r402_protocol::payment::{SettleRequest, SettleResponse, VerifyRequest, VerifyResponse};

/// Inspect JSON before `Facilitator::verify`. `None` means the handler should run.
pub(super) fn classify_verify_envelope(request: &VerifyRequest) -> Option<VerifyResponse> {
    if request.scheme_slug().is_some() {
        return None;
    }
    Some(VerifyResponse::invalid(
        None,
        envelope_reason(&request_json(request)),
    ))
}

/// Inspect JSON before `Facilitator::settle`.
pub(super) fn classify_settle_envelope(request: &SettleRequest) -> Option<SettleResponse> {
    if request.scheme_slug().is_some() {
        return None;
    }
    let reason = envelope_reason(&request_json(request));
    let error = r402_protocol::error::FacilitatorError::from(
        r402_protocol::error::VerificationError::from_wire(reason.as_str()),
    );
    SettleResponse::from_facilitator_error(&error, request.network(), "")
}

fn request_json<T: serde::Serialize>(request: &T) -> serde_json::Value {
    serde_json::to_value(request).unwrap_or(serde_json::Value::Null)
}

/// Version `2` with a bad `accepted` is `invalid_payload`; anything else is `invalid_x402_version`.
fn envelope_reason(json: &serde_json::Value) -> ErrorReason {
    if json.get("x402Version").and_then(serde_json::Value::as_u64) == Some(2) {
        ErrorReason::InvalidPayload
    } else {
        ErrorReason::InvalidX402Version
    }
}

#[cfg(test)]
#[allow(clippy::panic, clippy::indexing_slicing, reason = "unit tests")]
mod tests {
    use serde_json::json;

    use super::*;

    fn v2(network: &str, scheme: &str) -> serde_json::Value {
        json!({
            "x402Version": 2,
            "paymentPayload": {
                "accepted": { "network": network, "scheme": scheme }
            },
            "paymentRequirements": { "network": network }
        })
    }

    #[test]
    fn well_formed_slug_is_none() {
        let req = VerifyRequest::from(v2("eip155:84532", "exact"));
        assert!(
            classify_verify_envelope(&req).is_none(),
            "slug present → handler"
        );
        let settle = SettleRequest::from(v2("eip155:84532", "exact"));
        assert!(
            classify_settle_envelope(&settle).is_none(),
            "slug present → handler"
        );
    }

    #[test]
    fn version_not_2_is_invalid_x402_version() {
        let mut body = v2("eip155:84532", "exact");
        body["x402Version"] = json!(1);
        let resp =
            classify_verify_envelope(&VerifyRequest::from(body.clone())).expect("classified");
        match resp {
            VerifyResponse::Invalid {
                reason: Some(ErrorReason::InvalidX402Version),
                ..
            } => {}
            other => panic!("expected invalid_x402_version, got {other:?}"),
        }
        let settle = classify_settle_envelope(&SettleRequest::from(body)).expect("classified");
        match settle {
            SettleResponse::Failure {
                reason: ErrorReason::InvalidX402Version,
                ..
            } => {}
            other => panic!("expected invalid_x402_version, got {other:?}"),
        }
    }

    #[test]
    fn version_2_bad_accepted_is_invalid_payload() {
        let body = json!({
            "x402Version": 2,
            "paymentPayload": {},
            "paymentRequirements": { "network": "eip155:84532" }
        });
        let resp =
            classify_verify_envelope(&VerifyRequest::from(body.clone())).expect("classified");
        match resp {
            VerifyResponse::Invalid {
                reason: Some(ErrorReason::InvalidPayload),
                ..
            } => {}
            other => panic!("expected invalid_payload, got {other:?}"),
        }
        let settle = classify_settle_envelope(&SettleRequest::from(body)).expect("classified");
        match settle {
            SettleResponse::Failure {
                reason: ErrorReason::InvalidPayload,
                ..
            } => {}
            other => panic!("expected invalid_payload, got {other:?}"),
        }
    }
}
