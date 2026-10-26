//! Filesystem locations shared across services.
//!
//! Each directory is overridable through an environment variable so a
//! deployment never depends on the process working directory.

use std::path::PathBuf;

/// Resolves `var` from the environment, falling back to `default`.
pub fn dir_from_env(var: &str, default: &str) -> PathBuf {
    PathBuf::from(std::env::var(var).unwrap_or_else(|_| default.to_string()))
}
