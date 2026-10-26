//! `GET /coverage` — ATT&CK technique coverage from active detection content.
//!
//! Scans `detection-content/active/*.yml`, collects Sigma `tags` matching
//! `attack.t####(.###)` and reports the distinct technique list.

use std::collections::BTreeSet;

use axum::{http::StatusCode, response::IntoResponse, Json};
use serde_json::{json, Value};

/// Directory holding deployed Sigma rules.
const ACTIVE_DIR: &str = "detection-content/active";

/// Lists the distinct ATT&CK technique ids covered by active rules.
pub async fn get_coverage() -> Result<impl IntoResponse, (StatusCode, String)> {
    let mut techniques = BTreeSet::new();
    let mut rules = 0usize;
    let mut errors: Vec<String> = Vec::new();

    let entries = match std::fs::read_dir(ACTIVE_DIR) {
        Ok(e) => e,
        Err(e) => {
            return Ok(Json(json!({
                "techniques": Vec::<String>::new(),
                "rule_count": 0,
                "note": format!("cannot read {ACTIVE_DIR}: {e}"),
            })))
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
        rules += 1;
        match std::fs::read_to_string(&path)
            .map_err(anyhow::Error::from)
            .and_then(|t| serde_yaml_ng::from_str::<Value>(&t).map_err(Into::into))
        {
            Ok(doc) => {
                if let Some(tags) = doc.get("tags").and_then(Value::as_array) {
                    for tag in tags.iter().filter_map(Value::as_str) {
                        if let Some(t) = tag.strip_prefix("attack.t") {
                            techniques.insert(format!("T{t}").to_ascii_uppercase());
                        }
                    }
                }
            }
            Err(e) => errors.push(format!("{}: {e}", path.display())),
        }
    }

    Ok(Json(json!({
        "techniques": techniques.into_iter().collect::<Vec<_>>(),
        "rule_count": rules,
        "errors": errors,
    })))
}
