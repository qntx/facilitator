//! Command implementations for the facilitator CLI.
//!
//! Each subcommand lives in its own submodule:
//!
//! - [`init`] — Generate a default TOML configuration file.
//! - [`serve`] — Start the facilitator HTTP server.

pub mod init;
pub mod serve;
