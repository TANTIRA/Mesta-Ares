//! Operator kill switch: one predicate shared by agents and the policy gate.

use std::path::{Path, PathBuf};

/// Name of the kill file inside the policies directory.
pub const KILL_FILE: &str = "kill";

/// Resolves the policies directory (`$POLICIES_DIR`, default `policies`).
pub fn policies_dir() -> PathBuf {
    PathBuf::from(std::env::var("POLICIES_DIR").unwrap_or_else(|_| "policies".to_string()))
}

/// `true` when `ARES_KILL_SWITCH` is set to a truthy value.
pub fn kill_env_engaged() -> bool {
    std::env::var("ARES_KILL_SWITCH")
        .map(|v| is_truthy(&v))
        .unwrap_or(false)
}

/// `true` when `dir/kill` exists with non-whitespace content.
///
/// An existing but unreadable file counts as engaged (fail-closed).
pub fn kill_file_engaged_at(dir: &Path) -> bool {
    match std::fs::read_to_string(dir.join(KILL_FILE)) {
        Ok(content) => !content.trim().is_empty(),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => false,
        Err(_) => true,
    }
}

/// Live check: `ARES_KILL_SWITCH` or a non-empty `$POLICIES_DIR/kill`.
pub fn kill_switch_engaged() -> bool {
    kill_env_engaged() || kill_file_engaged_at(&policies_dir())
}

fn is_truthy(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "on"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn falsy_env_values_do_not_engage() {
        for v in ["0", "false", "off", ""] {
            assert!(!is_truthy(v));
        }
        for v in ["1", "true", "YES", " on "] {
            assert!(is_truthy(v));
        }
    }

    #[test]
    fn kill_file_engages_only_on_content() {
        let dir = std::env::temp_dir().join(format!("ares-kill-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        assert!(!kill_file_engaged_at(&dir), "absent file disengages");
        std::fs::write(dir.join(KILL_FILE), "  \n").unwrap();
        assert!(!kill_file_engaged_at(&dir), "whitespace disengages");
        std::fs::write(dir.join(KILL_FILE), "engaged\n").unwrap();
        assert!(kill_file_engaged_at(&dir), "content engages");
        std::fs::remove_dir_all(&dir).ok();
    }
}
