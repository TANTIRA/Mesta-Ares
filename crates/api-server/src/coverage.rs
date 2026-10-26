//! `GET /coverage` — ATT&CK technique coverage from active detection content.
//!
//! Scans `detection-content/active/*.yml`, collects Sigma `tags` matching
//! `attack.t####(.###)` and reports the distinct technique list.

use std::collections::BTreeSet;

use axum::{response::IntoResponse, Json};
use serde_json::{json, Value};

use crate::error::ApiError;

/// Lists the distinct ATT&CK technique ids covered by active rules.
pub async fn get_coverage() -> Result<impl IntoResponse, ApiError> {
    let mut techniques = BTreeSet::new();
    let mut rules = 0usize;
    let mut errors: Vec<String> = Vec::new();

    let dir = agent_protocol::paths::dir_from_env("DETECTION_ACTIVE_DIR", "detection-content/active");
    let (docs, load_errors) = crate::yaml_dir::load(&dir);
    errors.extend(load_errors);
    if docs.is_empty() && !errors.is_empty() {
        return Ok(Json(json!({
            "techniques": Vec::<String>::new(),
            "rule_count": 0,
            "note": errors.join("; "),
        })));
    }

    for (_stem, doc) in docs {
        rules += 1;
        if let Some(tags) = doc.get("tags").and_then(Value::as_array) {
            for tag in tags.iter().filter_map(Value::as_str) {
                if let Some(t) = tag.strip_prefix("attack.t") {
                    techniques.insert(format!("T{t}").to_ascii_uppercase());
                }
            }
        }
    }

    Ok(Json(json!({
        "techniques": techniques.into_iter().collect::<Vec<_>>(),
        "rule_count": rules,
        "errors": errors,
    })))
}
