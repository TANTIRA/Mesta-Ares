//! Operator kill switch: one predicate shared by agents and the policy gate.
//!
//! State is read from the first source that applies, in this order:
//!
//! 1. `ARES_KILL_SWITCH` env (`1`/`true`/`yes`/`on`) — boot-time override.
//! 2. `ARES_KILL_FILE` — explicit path to the kill file (absolute or CWD-relative).
//! 3. `$ARES_CONTROL_DIR/kill` — the shipped control surface. api-server mounts
//!    this directory read-write (it is the only writer); tool-server and every
//!    agent mount it read-only, so the state written by
//!    `POST /controls/kill-switch` is the state the gate actually observes.
//! 4. `$POLICIES_DIR/kill` — legacy fallback (and what a bare local checkout
//!    uses when no control dir is configured).
//!
//! Sources 2–4 are files; an existing but unreadable file counts as engaged
//! (fail-closed).

use std::path::{Path, PathBuf};

/// Name of the kill file inside the control directory.
pub const KILL_FILE: &str = "kill";

const ENV_KILL_SWITCH: &str = "ARES_KILL_SWITCH";
const ENV_KILL_FILE: &str = "ARES_KILL_FILE";
const ENV_CONTROL_DIR: &str = "ARES_CONTROL_DIR";
const ENV_POLICIES_DIR: &str = "POLICIES_DIR";

/// Which source an engaged state came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KillSource {
    /// `ARES_KILL_SWITCH` is truthy.
    Env,
    /// The kill file exists with non-whitespace content.
    File,
    /// Nothing is engaged.
    Off,
}

impl KillSource {
    /// Stable label used by the control API and logs.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Env => "env",
            Self::File => "file",
            Self::Off => "off",
        }
    }
}

/// Resolves the policies directory (`$POLICIES_DIR`, default `policies`).
pub fn policies_dir() -> PathBuf {
    policies_dir_from(|name| std::env::var(name).ok())
}

/// Resolves the control directory (`$ARES_CONTROL_DIR`, default `$POLICIES_DIR`).
pub fn control_dir() -> PathBuf {
    control_dir_from(|name| std::env::var(name).ok())
}

/// Resolves the kill file (`$ARES_KILL_FILE`, else `$ARES_CONTROL_DIR/kill`).
pub fn kill_file() -> PathBuf {
    kill_file_from(|name| std::env::var(name).ok())
}

/// `true` when `ARES_KILL_SWITCH` is set to a truthy value.
pub fn kill_env_engaged() -> bool {
    std::env::var(ENV_KILL_SWITCH)
        .map(|v| is_truthy(&v))
        .unwrap_or(false)
}

/// `true` when `path` exists with non-whitespace content.
///
/// An existing but unreadable file counts as engaged (fail-closed).
pub fn kill_file_engaged_path(path: &Path) -> bool {
    match std::fs::read_to_string(path) {
        Ok(content) => !content.trim().is_empty(),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => false,
        Err(_) => true,
    }
}

/// `true` when the configured kill file is engaged.
pub fn kill_file_engaged() -> bool {
    kill_file_engaged_path(&kill_file())
}

/// `true` when `dir/kill` exists with non-whitespace content.
///
/// Kept for callers that already hold a directory (tests, policy fixtures).
pub fn kill_file_engaged_at(dir: &Path) -> bool {
    kill_file_engaged_path(&dir.join(KILL_FILE))
}

/// Live check: env override or the configured kill file.
pub fn kill_switch_engaged() -> bool {
    kill_env_engaged() || kill_file_engaged()
}

/// Which source currently drives the switch (for `/controls/kill-switch`).
pub fn kill_switch_source() -> KillSource {
    if kill_env_engaged() {
        KillSource::Env
    } else if kill_file_engaged() {
        KillSource::File
    } else {
        KillSource::Off
    }
}

fn non_empty(value: Option<String>) -> Option<String> {
    value
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

fn policies_dir_from(get: impl Fn(&str) -> Option<String>) -> PathBuf {
    non_empty(get(ENV_POLICIES_DIR))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("policies"))
}

fn control_dir_from(get: impl Fn(&str) -> Option<String>) -> PathBuf {
    non_empty(get(ENV_CONTROL_DIR))
        .map(PathBuf::from)
        .unwrap_or_else(|| policies_dir_from(&get))
}

fn kill_file_from(get: impl Fn(&str) -> Option<String>) -> PathBuf {
    if let Some(path) = non_empty(get(ENV_KILL_FILE)) {
        return PathBuf::from(path);
    }
    control_dir_from(get).join(KILL_FILE)
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

    fn getter<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        move |name| {
            pairs
                .iter()
                .find(|(key, _)| *key == name)
                .map(|(_, value)| value.to_string())
        }
    }

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

    #[test]
    fn kill_file_resolution_precedence() {
        let explicit_pairs = [
            (ENV_KILL_FILE, "/run/ares/kill"),
            (ENV_CONTROL_DIR, "/control"),
            (ENV_POLICIES_DIR, "/policies"),
        ];
        assert_eq!(
            kill_file_from(getter(&explicit_pairs)),
            PathBuf::from("/run/ares/kill")
        );

        // Otherwise the shared control dir wins over the policies dir.
        let control_pairs = [
            (ENV_CONTROL_DIR, "/control"),
            (ENV_POLICIES_DIR, "/policies"),
        ];
        assert_eq!(
            kill_file_from(getter(&control_pairs)),
            PathBuf::from("/control/kill")
        );

        // No control dir: fall back to the policies dir.
        let policies_pairs = [(ENV_POLICIES_DIR, "/policies")];
        assert_eq!(
            kill_file_from(getter(&policies_pairs)),
            PathBuf::from("/policies/kill")
        );

        // Nothing configured: relative default, matching a bare checkout.
        let empty: [(&str, &str); 0] = [];
        assert_eq!(
            kill_file_from(getter(&empty)),
            PathBuf::from("policies/kill")
        );

        // Blank values are ignored rather than resolving to the CWD.
        let blank_pairs = [
            (ENV_KILL_FILE, "  "),
            (ENV_CONTROL_DIR, ""),
            (ENV_POLICIES_DIR, " /policies "),
        ];
        assert_eq!(
            kill_file_from(getter(&blank_pairs)),
            PathBuf::from("/policies/kill")
        );
    }

    #[test]
    fn control_dir_defaults_to_policies_dir() {
        let policies_pairs = [(ENV_POLICIES_DIR, "/etc/ares/policies")];
        assert_eq!(
            control_dir_from(getter(&policies_pairs)),
            PathBuf::from("/etc/ares/policies")
        );

        let explicit_pairs = [
            (ENV_CONTROL_DIR, "/var/run/ares"),
            (ENV_POLICIES_DIR, "/etc/ares/policies"),
        ];
        assert_eq!(
            control_dir_from(getter(&explicit_pairs)),
            PathBuf::from("/var/run/ares")
        );
    }
}
