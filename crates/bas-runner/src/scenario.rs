//! BAS scenario manifests loaded from `bas/scenarios/*.yml`.

use anyhow::Context;
use serde::Deserialize;
use std::path::PathBuf;

/// Directory scanned for scenario manifests.
pub const SCENARIOS_DIR: &str = "bas/scenarios";

/// A breach-and-attack-simulation scenario manifest.
#[derive(Debug, Deserialize)]
pub struct Scenario {
    /// Scenario identifier (e.g. `bas-t1003-001`).
    pub id: String,
    /// Human-readable scenario name.
    pub name: String,
    /// ATT&CK technique ids exercised by the scenario (`T####[.###]`).
    #[serde(default)]
    pub techniques: Vec<String>,
    /// Fleet group the executor targets (informational).
    #[serde(default)]
    pub target: String,
    /// How the scenario executes on the test fleet.
    pub executor: Executor,
}

/// Executor block of a scenario manifest.
#[derive(Debug, Deserialize)]
pub struct Executor {
    /// Executor platform: `ssh`/`linux` uses `BAS_SSH_TARGET`,
    /// `winrm`/`windows` uses `BAS_WINRM_TARGET`.
    pub platform: String,
    /// Command line run on the fleet target.
    pub command: String,
}

/// Loads every `.yml`/`.yaml` file under [`SCENARIOS_DIR`].
pub fn load_all() -> anyhow::Result<Vec<(PathBuf, Scenario)>> {
    let mut out = Vec::new();
    let entries = match std::fs::read_dir(SCENARIOS_DIR) {
        Ok(e) => e,
        Err(e) => {
            tracing::warn!(error = %e, dir = SCENARIOS_DIR, "scenario dir unreadable");
            return Ok(out);
        }
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !matches!(
            path.extension().and_then(|e| e.to_str()),
            Some("yml") | Some("yaml")
        ) {
            continue;
        }
        let text =
            std::fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
        let scenario: Scenario =
            serde_yaml_ng::from_str(&text).with_context(|| format!("parse {}", path.display()))?;
        out.push((path, scenario));
    }
    out.sort_by(|a, b| a.1.id.cmp(&b.1.id));
    Ok(out)
}
