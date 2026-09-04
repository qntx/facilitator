//! Named secret sources (`env` | `file`). Never TOML literals.

use std::fmt;
use std::path::PathBuf;

use serde::Deserialize;

use crate::error::Error;

/// How a file secret is encoded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum KeyEncoding {
    /// EVM secp256k1; Aptos/TVM hex.
    Hex,
    /// SVM 64-byte keypair.
    Base58,
    /// NEAR `ed25519:…`, Stellar `S…`, Hedera DER/hex as text.
    Utf8,
    /// Algorand/Keeta seeds.
    Base64,
}

/// Where a named `[signer.*]` secret is read from.
#[derive(Clone, PartialEq, Eq, Deserialize)]
#[serde(tag = "source", deny_unknown_fields)]
pub enum SecretSource {
    /// Environment variable. The value is the variable name, not the secret.
    #[serde(rename = "env")]
    Env {
        /// Process environment variable name.
        env: String,
    },
    /// File path read at startup.
    #[serde(rename = "file")]
    File {
        /// Path to the secret file.
        path: PathBuf,
        /// Encoding of the file contents.
        encoding: KeyEncoding,
    },
}

impl fmt::Debug for SecretSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.describe())
    }
}

impl SecretSource {
    /// Operator-facing description that never includes secret material.
    #[must_use]
    pub fn describe(&self) -> String {
        match self {
            Self::Env { env } => format!("source=env({env})"),
            Self::File { path, encoding } => {
                format!("source=file({}) encoding={encoding:?}", path.display())
            }
        }
    }

    /// Resolve the secret using `lookup` for `env` sources.
    ///
    /// # Errors
    ///
    /// Missing environment variable, unreadable file, or empty contents.
    pub fn resolve(&self, lookup: &impl Fn(&str) -> Option<String>) -> Result<String, Error> {
        match self {
            Self::Env { env } => lookup(env).ok_or_else(|| {
                Error::secret(format!("env var '{env}' not found ({})", self.describe()))
            }),
            Self::File { path, encoding: _ } => {
                warn_if_world_readable(path);
                let raw = std::fs::read_to_string(path).map_err(|err| {
                    Error::secret_with(format!("failed to read '{}'", path.display()), err)
                })?;
                let trimmed = raw.trim();
                if trimmed.is_empty() {
                    return Err(Error::secret(format!(
                        "secret file '{}' is empty",
                        path.display()
                    )));
                }
                Ok(trimmed.to_owned())
            }
        }
    }
}

/// Warn when a Unix secret file is group- or world-readable.
fn warn_if_world_readable(path: &std::path::Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(meta) = std::fs::metadata(path) {
            let mode = meta.permissions().mode();
            if mode & 0o077 != 0 {
                tracing::warn!(
                    path = %path.display(),
                    mode = format!("{mode:o}"),
                    "secret file is group- or world-readable; prefer 0o600"
                );
            }
        }
    }
    #[cfg(not(unix))]
    {
        let _ = path;
    }
}
