//! Filesystem locations used by the read models.
//!
//! Every directory is overridable so a deployment can point the read models at
//! its own layout instead of relying on the process working directory.

use std::path::PathBuf;

/// Resolves `var` from the environment, falling back to `default`.
pub fn dir_from_env(var: &str, default: &str) -> PathBuf {
    PathBuf::from(std::env::var(var).unwrap_or_else(|_| default.to_string()))
}
