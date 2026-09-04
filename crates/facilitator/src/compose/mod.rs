//! Private `FacilitatorMap` dispatching by `SchemeSlug`.

#[cfg(feature = "evm")]
mod evm;

use std::collections::HashMap;
use std::sync::Arc;

use compact_str::CompactString;
use r402_facilitator::{DynFacilitator, Facilitator};
use r402_protocol::ChainId;
use r402_protocol::error::{FacilitatorError, VerificationError};
use r402_protocol::payment::{
    SettleRequest, SettleResponse, SupportedResponse, VerifyRequest, VerifyResponse,
};
use r402_protocol::scheme::SchemeSlug;

use crate::config::{Config, Network};
use crate::error::Error;

/// In-process scheme handlers keyed by `SchemeSlug`.
pub struct FacilitatorMap {
    /// Lookup by dispatch key.
    handlers: HashMap<SchemeSlug, Arc<dyn DynFacilitator>>,
    /// Insertion order for `/supported` kinds.
    order: Vec<SchemeSlug>,
    /// Extra extension identifiers appended after handler unions.
    extra_extensions: Vec<String>,
}

impl std::fmt::Debug for FacilitatorMap {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FacilitatorMap")
            .field("slugs", &self.order)
            .field("extra_extensions", &self.extra_extensions)
            .finish_non_exhaustive()
    }
}

impl Default for FacilitatorMap {
    fn default() -> Self {
        Self::new()
    }
}

impl FacilitatorMap {
    /// Empty map. `GET /supported` returns `kinds: []`.
    #[must_use]
    pub fn new() -> Self {
        Self {
            handlers: HashMap::new(),
            order: Vec::new(),
            extra_extensions: Vec::new(),
        }
    }

    /// Whether any handler is registered.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.handlers.is_empty()
    }

    /// Register a handler. Duplicate slugs are an error.
    ///
    /// # Errors
    ///
    /// Duplicate `SchemeSlug`.
    pub fn insert(
        &mut self,
        slug: SchemeSlug,
        handler: Arc<dyn DynFacilitator>,
    ) -> Result<(), Error> {
        if self.handlers.contains_key(&slug) {
            return Err(Error::config(format!("duplicate handler for {slug}")));
        }
        self.order.push(slug.clone());
        self.handlers.insert(slug, handler);
        Ok(())
    }

    /// Append a config-driven extension identifier (SDK `supported()` does not).
    pub fn push_extension(&mut self, identifier: impl Into<String>) {
        self.extra_extensions.push(identifier.into());
    }
}

impl Facilitator for FacilitatorMap {
    async fn verify(&self, request: VerifyRequest) -> Result<VerifyResponse, FacilitatorError> {
        let slug = lookup_slug(request.scheme_slug())?;
        let handler = handler_for(self, &slug)?;
        DynFacilitator::verify(handler.as_ref(), request).await
    }

    async fn settle(&self, request: SettleRequest) -> Result<SettleResponse, FacilitatorError> {
        let slug = lookup_slug(request.scheme_slug())?;
        let handler = handler_for(self, &slug)?;
        DynFacilitator::settle(handler.as_ref(), request).await
    }

    async fn supported(&self) -> Result<SupportedResponse, FacilitatorError> {
        merge_supported(self).await
    }
}

/// Construct in-process scheme handlers from `config`.
///
/// Uses `with_settlement_cache` as a constructor (never `try_new`) so EVM
/// exact and upto handlers share the process [`r402_facilitator::SettlementCache`].
/// A listed scheme without a constructor in this build is an error. The
/// returned map is nonempty.
///
/// # Errors
///
/// Unresolvable secrets, invalid keys, provider construction failure, a listed
/// scheme this build cannot construct, or an empty map.
pub fn build(
    config: &Config,
    lookup: &impl Fn(&str) -> Option<String>,
) -> Result<FacilitatorMap, Error> {
    let mut map = FacilitatorMap::new();
    #[cfg(feature = "evm")]
    let evm = evm::Prepare::new(config)?;
    for network in &config.networks {
        match network {
            Network::Evm(net) => {
                #[cfg(feature = "evm")]
                evm.register(&mut map, net, config, lookup)?;
                #[cfg(not(feature = "evm"))]
                reject_unconstructed(&net.chain_id, &net.schemes)?;
            }
            Network::Svm(net) => reject_unconstructed(&net.chain_id, &net.schemes)?,
        }
    }
    #[cfg(feature = "evm")]
    evm.finish(&mut map);
    require_nonempty(&map)?;
    Ok(map)
}

fn lookup_slug(slug: Option<SchemeSlug>) -> Result<SchemeSlug, FacilitatorError> {
    slug.ok_or_else(|| VerificationError::from_wire("invalid_x402_version").into())
}

fn handler_for<'a>(
    map: &'a FacilitatorMap,
    slug: &SchemeSlug,
) -> Result<&'a Arc<dyn DynFacilitator>, FacilitatorError> {
    map.handlers.get(slug).ok_or_else(|| {
        FacilitatorError::aborted(
            "no_facilitator_for_network",
            format!("no handler for {slug}"),
        )
    })
}

async fn merge_supported(map: &FacilitatorMap) -> Result<SupportedResponse, FacilitatorError> {
    let mut kinds = Vec::new();
    let mut extensions: Vec<CompactString> = Vec::new();
    let mut signers: Vec<(CompactString, Vec<CompactString>)> = Vec::new();
    for slug in &map.order {
        let Some(handler) = map.handlers.get(slug) else {
            continue;
        };
        let resp = DynFacilitator::supported(handler.as_ref()).await?;
        kinds.extend(resp.kinds);
        union_strings(&mut extensions, resp.extensions);
        union_signer_map(&mut signers, resp.signers);
    }
    for extra in &map.extra_extensions {
        let ident = CompactString::from(extra.as_str());
        if !extensions.contains(&ident) {
            extensions.push(ident);
        }
    }
    Ok(SupportedResponse::new()
        .with_kinds(kinds)
        .with_extensions(extensions)
        .with_signers(signers.into_iter().collect()))
}

fn union_strings(dest: &mut Vec<CompactString>, src: Vec<CompactString>) {
    for item in src {
        if !dest.contains(&item) {
            dest.push(item);
        }
    }
}

#[allow(
    clippy::implicit_hasher,
    reason = "wire signers map has no hasher contract"
)]
fn union_signer_map(
    dest: &mut Vec<(CompactString, Vec<CompactString>)>,
    src: HashMap<CompactString, Vec<CompactString>>,
) {
    for (key, addrs) in src {
        append_signer_addrs(dest, key, addrs);
    }
}

/// Merge addresses into an existing CAIP-2 signer list, or append a new key.
fn append_signer_addrs(
    dest: &mut Vec<(CompactString, Vec<CompactString>)>,
    key: CompactString,
    addrs: Vec<CompactString>,
) {
    if let Some((_, existing)) = dest.iter_mut().find(|(k, _)| *k == key) {
        extend_unique(existing, addrs);
        return;
    }
    dest.push((key, addrs));
}

/// Append addresses that are not already present.
fn extend_unique(existing: &mut Vec<CompactString>, addrs: Vec<CompactString>) {
    for addr in addrs {
        if !existing.contains(&addr) {
            existing.push(addr);
        }
    }
}

fn reject_unconstructed(chain_id: &ChainId, schemes: &[String]) -> Result<(), Error> {
    let Some(name) = schemes.first() else {
        return Err(Error::config(format!(
            "[network.\"{chain_id}\"] `schemes` must not be empty"
        )));
    };
    Err(scheme_not_enabled(name, chain_id))
}

fn scheme_not_enabled(name: &str, chain_id: &ChainId) -> Error {
    Error::config(format!(
        "scheme '{name}' on {chain_id} is not enabled in this build"
    ))
}

fn require_nonempty(map: &FacilitatorMap) -> Result<(), Error> {
    if map.is_empty() {
        return Err(Error::config(
            "nonempty constructed map required; no scheme handlers were registered",
        ));
    }
    Ok(())
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "unit tests"
)]
mod tests {
    use std::str::FromStr;

    use super::*;

    #[test]
    fn empty_map_is_startup_error() {
        let err = require_nonempty(&FacilitatorMap::new()).expect_err("empty");
        assert!(
            err.to_string()
                .contains("nonempty constructed map required"),
            "got {err}"
        );
    }

    #[test]
    fn listed_scheme_without_constructor_is_startup_error() {
        let chain = ChainId::from_str("eip155:84532").expect("caip-2");
        let err = scheme_not_enabled("auth-capture", &chain);
        assert!(
            err.to_string()
                .contains("scheme 'auth-capture' on eip155:84532 is not enabled in this build"),
            "got {err}"
        );
    }
}
