//! Operator kill switch for halting tool execution.

use std::path::Path;

/// Report whether the operator kill switch is engaged.
///
/// Engaged when either holds:
/// - `ARES_KILL_SWITCH` is set to a truthy value (`1`, `true`, `yes`, `on`).
/// - The file `$POLICIES_DIR/kill` (default `policies/kill`) exists and is
///   non-empty.
pub fn kill_switch_engaged() -> bool {
    if std::env::var("ARES_KILL_SWITCH")
        .map(|v| is_truthy(&v))
        .unwrap_or(false)
    {
        return true;
    }
    kill_file_engaged()
}

fn is_truthy(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "on"
    )
}

fn kill_file_engaged() -> bool {
    let dir = std::env::var("POLICIES_DIR").unwrap_or_else(|_| "policies".to_string());
    let path = Path::new(&dir).join("kill");
    std::fs::metadata(&path)
        .map(|m| m.is_file() && m.len() > 0)
        .unwrap_or(false)
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
}
