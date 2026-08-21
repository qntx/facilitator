//! EIP-155 chain handle and provider construction.

use std::sync::Arc;

use alloy_network::EthereumWallet;
use alloy_signer_local::PrivateKeySigner;
use r402_core::chain::{ChainId, ChainProvider};
use r402_core::facilitator::DynFacilitator;
use r402_core::scheme::{SchemeBuilder, SchemeId, SchemeRegistry};
use r402_evm::chain::{Eip155ChainProvider, Eip155ChainReference};
use r402_evm::{Eip155BatchSettlement, Eip155Exact, Eip155Upto};
use serde::Deserialize;
use url::Url;

use crate::error::AppError;

/// Local handle: r402-evm has no `SchemeBuilder<&Eip155ChainProvider>`, which `register` requires.
#[derive(Debug)]
pub(crate) struct Eip155Handle(pub Arc<Eip155ChainProvider>);

impl ChainProvider for Eip155Handle {
    fn signer_addresses(&self) -> Vec<String> {
        self.0.signer_addresses()
    }

    fn chain_id(&self) -> ChainId {
        self.0.chain_id()
    }
}

impl SchemeBuilder<&Eip155Handle> for Eip155Exact {
    fn build(
        &self,
        provider: &Eip155Handle,
        config: Option<serde_json::Value>,
    ) -> Result<Box<dyn DynFacilitator>, Box<dyn std::error::Error + Send + Sync>> {
        SchemeBuilder::<Arc<Eip155ChainProvider>>::build(self, Arc::clone(&provider.0), config)
    }
}

impl SchemeBuilder<&Eip155Handle> for Eip155Upto {
    fn build(
        &self,
        provider: &Eip155Handle,
        config: Option<serde_json::Value>,
    ) -> Result<Box<dyn DynFacilitator>, Box<dyn std::error::Error + Send + Sync>> {
        SchemeBuilder::<Arc<Eip155ChainProvider>>::build(self, Arc::clone(&provider.0), config)
    }
}

impl SchemeBuilder<&Eip155Handle> for Eip155BatchSettlement {
    fn build(
        &self,
        provider: &Eip155Handle,
        config: Option<serde_json::Value>,
    ) -> Result<Box<dyn DynFacilitator>, Box<dyn std::error::Error + Send + Sync>> {
        SchemeBuilder::<Arc<Eip155ChainProvider>>::build(self, Arc::clone(&provider.0), config)
    }
}

/// Scheme names `register_eip155_schemes` registers for each EIP-155 chain.
///
/// Extra schemes are registration-only Cargo features; auth-capture is never listed.
#[cfg(test)]
#[must_use]
fn eip155_registered_scheme_names() -> Vec<&'static str> {
    let mut names = vec![Eip155Exact.scheme()];
    #[cfg(feature = "scheme-upto")]
    names.push(Eip155Upto.scheme());
    #[cfg(feature = "scheme-batch-settlement")]
    names.push(Eip155BatchSettlement.scheme());
    names
}

/// Register compiled EVM schemes for one chain handle.
///
/// # Errors
///
/// Returns an error if a scheme blueprint fails to build.
pub(crate) fn register_eip155_schemes(
    registry: &mut SchemeRegistry,
    handle: &Eip155Handle,
) -> Result<(), AppError> {
    registry.register(&Eip155Exact, handle, None).map_err(|e| {
        AppError::chain(format!(
            "failed to register eip155 {}: {e}",
            Eip155Exact.scheme()
        ))
    })?;
    #[cfg(feature = "scheme-upto")]
    registry.register(&Eip155Upto, handle, None).map_err(|e| {
        AppError::chain(format!(
            "failed to register eip155 {}: {e}",
            Eip155Upto.scheme()
        ))
    })?;
    #[cfg(feature = "scheme-batch-settlement")]
    registry
        .register(&Eip155BatchSettlement, handle, None)
        .map_err(|e| {
            AppError::chain(format!(
                "failed to register eip155 {}: {e}",
                Eip155BatchSettlement.scheme()
            ))
        })?;
    Ok(())
}

/// Single RPC endpoint entry for EVM chains.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Eip155RpcEndpoint {
    /// HTTP(S) RPC URL.
    pub http: String,
    /// Optional per-endpoint rate limit (requests/second).
    #[serde(default)]
    pub rate_limit: Option<u32>,
}

/// Inner configuration for an EVM chain (matches TOML structure).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Eip155ChainConfigInner {
    /// RPC endpoint(s).
    pub rpc: Vec<Eip155RpcEndpoint>,
    /// Signer private keys (hex, 0x-prefixed). Injected by the signers preprocessor.
    #[serde(default)]
    pub signers: Vec<String>,
    /// Whether the chain supports EIP-1559 gas pricing (default: true).
    #[serde(default = "default_true")]
    pub eip1559: bool,
    /// Whether the chain supports flashblocks (default: false).
    #[serde(default)]
    pub flashblocks: bool,
    /// Transaction receipt timeout in seconds (default: 20).
    #[serde(default = "default_receipt_timeout")]
    pub receipt_timeout_secs: u64,
}

/// Serde default returning `true` (for EIP-1559 opt-in).
const fn default_true() -> bool {
    true
}

/// Default EVM receipt wait; must finish inside the 30 s HTTP client budget.
const fn default_receipt_timeout() -> u64 {
    20
}

/// Full EVM chain configuration with chain reference.
#[derive(Debug, Clone)]
pub(crate) struct Eip155ChainConfig {
    /// Numeric EIP-155 chain reference.
    pub chain_reference: Eip155ChainReference,
    /// TOML-level configuration.
    pub inner: Eip155ChainConfigInner,
}

impl Eip155ChainConfig {
    /// Parse a CAIP-2 keyed chain table.
    ///
    /// # Errors
    ///
    /// Returns an error if the reference is not a numeric EIP-155 id or the
    /// table does not match [`Eip155ChainConfigInner`].
    pub(crate) fn from_toml(chain_id: &ChainId, value: toml::Value) -> Result<Self, AppError> {
        let chain_reference = Eip155ChainReference::try_from(chain_id)
            .map_err(|e| AppError::config_with(format!("invalid chain id '{chain_id}'"), e))?;
        let inner: Eip155ChainConfigInner = value.try_into().map_err(|e: toml::de::Error| {
            AppError::config_with(format!("invalid [chains.\"{chain_id}\"]"), e)
        })?;
        Ok(Self {
            chain_reference,
            inner,
        })
    }

    /// Returns the CAIP-2 chain ID for this configuration.
    #[must_use]
    pub(crate) fn chain_id(&self) -> ChainId {
        self.chain_reference.into()
    }
}

/// Build an EIP-155 handle from TOML.
///
/// # Errors
///
/// Returns an error if signer keys cannot be parsed, no signers are
/// configured, or no HTTP RPC remains after URL filtering.
pub(crate) fn build_eip155_handle(config: &Eip155ChainConfig) -> Result<Eip155Handle, AppError> {
    let chain_id = config.chain_id();
    if config.inner.signers.is_empty() {
        return Err(AppError::chain(format!(
            "no signers configured for EVM chain {chain_id}"
        )));
    }

    let mut parsed = Vec::with_capacity(config.inner.signers.len());
    for key in &config.inner.signers {
        let signer = key
            .parse::<PrivateKeySigner>()
            .map_err(|e| AppError::chain_with("failed to parse EVM signer key", e))?;
        parsed.push(signer);
    }
    let mut signers = parsed.into_iter();
    let first = signers.next().ok_or_else(|| {
        AppError::chain(format!("no signers configured for EVM chain {chain_id}"))
    })?;
    let mut wallet = EthereumWallet::from(first);
    for signer in signers {
        wallet.register_signer(signer);
    }

    let endpoints: Vec<(Url, Option<u32>)> = config
        .inner
        .rpc
        .iter()
        .filter_map(|ep| match Url::parse(&ep.http) {
            Ok(url) => Some((url, ep.rate_limit)),
            Err(e) => {
                tracing::warn!(rpc_url = %ep.http, error = %e, "Skipping invalid RPC URL");
                None
            }
        })
        .collect();
    if endpoints.is_empty() {
        return Err(AppError::chain(format!(
            "no HTTP RPC remaining for EVM chain {chain_id}"
        )));
    }

    let provider = Eip155ChainProvider::new(
        config.chain_reference,
        wallet,
        &endpoints,
        config.inner.eip1559,
        config.inner.flashblocks,
        config.inner.receipt_timeout_secs,
    )
    .map_err(|e| AppError::chain(format!("EVM provider init failed: {e}")))?;

    Ok(Eip155Handle(Arc::new(provider)))
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    use super::*;
    use crate::routes::{self, FacilitatorState};

    /// Anvil account #0 — never used on-chain; `supported` is local.
    const TEST_SIGNER: &str = "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80";

    fn test_handle() -> Eip155Handle {
        let config = Eip155ChainConfig {
            chain_reference: Eip155ChainReference::new(84532),
            inner: Eip155ChainConfigInner {
                rpc: vec![Eip155RpcEndpoint {
                    http: "http://127.0.0.1:9".to_owned(),
                    rate_limit: None,
                }],
                signers: vec![TEST_SIGNER.to_owned()],
                eip1559: true,
                flashblocks: false,
                receipt_timeout_secs: 20,
            },
        };
        build_eip155_handle(&config).expect("test Eip155Handle")
    }

    #[test]
    fn registered_scheme_names_match_official_ts_hosting() {
        let names = eip155_registered_scheme_names();
        assert!(names.contains(&"exact"), "exact is always registered");
        assert!(
            !names.contains(&"auth-capture"),
            "auth-capture is not hosted"
        );
        #[cfg(feature = "scheme-upto")]
        assert!(names.contains(&"upto"), "scheme-upto registers upto");
        #[cfg(not(feature = "scheme-upto"))]
        assert!(!names.contains(&"upto"), "scheme-upto off");
        #[cfg(feature = "scheme-batch-settlement")]
        assert!(
            names.contains(&"batch-settlement"),
            "scheme-batch-settlement registers batch-settlement"
        );
        #[cfg(not(feature = "scheme-batch-settlement"))]
        assert!(
            !names.contains(&"batch-settlement"),
            "scheme-batch-settlement off"
        );
    }

    #[tokio::test]
    async fn http_supported_matches_registered_schemes() {
        let handle = test_handle();
        let mut registry = SchemeRegistry::new();
        register_eip155_schemes(&mut registry, &handle).expect("register");
        let state: FacilitatorState = Arc::new(registry);
        let app = routes::routes().with_state(state);
        let response = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/supported")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("oneshot");
        assert_eq!(response.status(), StatusCode::OK, "GET /supported is 200");
        let bytes = response
            .into_body()
            .collect()
            .await
            .expect("body")
            .to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
        let kinds = json
            .get("kinds")
            .and_then(serde_json::Value::as_array)
            .expect("kinds");
        let mut got: Vec<&str> = kinds
            .iter()
            .map(|kind| {
                kind.get("scheme")
                    .and_then(serde_json::Value::as_str)
                    .expect("scheme")
            })
            .collect();
        got.sort_unstable();
        let mut expected = eip155_registered_scheme_names();
        expected.sort_unstable();
        assert_eq!(got, expected, "GET /supported schemes");
        assert!(!got.contains(&"auth-capture"), "auth-capture is not hosted");
        assert!(
            kinds
                .iter()
                .all(|kind| kind.get("x402Version").and_then(serde_json::Value::as_u64) == Some(2)),
            "kinds are v2"
        );
        #[cfg(feature = "scheme-upto")]
        assert!(got.contains(&"upto"), "/supported includes upto");
    }
}
