//! Blockchain chain types, configuration, and provider registry.
//!
//! - [`config`] — Chain configuration types and CAIP-2 keyed TOML (de)serialisation.
//! - [`provider`] — [`ChainProvider`] enum, trait impl, and registry construction.
//! - [`schemes`] — [`SchemeBuilder`] implementations bridging providers to scheme handlers.

mod config;
mod provider;
mod schemes;

pub use self::config::*;
pub use self::provider::*;
