//! `GET /playbooks` and `GET /playbooks/:name` — response playbooks on disk.
//!
//! Scans `PLAYBOOKS_DIR` (default `playbooks/`) for `*.yml`/`*.yaml` files
//! and returns each parsed document. A playbook is a graph document:
//! `name`, `description`, `trigger`, `nodes` (each with `id`, `kind`,
//! optional `label`/`tool`/`args`) and `edges` (`{from, to}`).

use axum::{extract::Path, http::StatusCode, Json};
use serde_json::Value;
use tracing::warn;

/// Directory scanned for playbook YAML files.
fn playbooks_dir() -> String {
    std::env::var("PLAYBOOKS_DIR").unwrap_or_else(|_| "playbooks".to_string())
}

/// Loads every playbook document in `PLAYBOOKS_DIR`, sorted by `name`.
/// Unparseable files are skipped and reported in `errors`.
fn load_playbooks() -> (Vec<Value>, Vec<String>) {
    let dir = playbooks_dir();
    let mut playbooks: Vec<Value> = Vec::new();
    let mut errors: Vec<String> = Vec::new();

    let entries = match std::fs::read_dir(&dir) {
        Ok(e) => e,
        Err(e) => {
            errors.push(format!("cannot read {dir}: {e}"));
            return (playbooks, errors);
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
        match std::fs::read_to_string(&path)
            .map_err(anyhow::Error::from)
            .and_then(|t| serde_yaml_ng::from_str::<Value>(&t).map_err(Into::into))
        {
            Ok(mut doc) => {
                if doc.is_object() {
                    // Keep the filename stem as a fallback key for lookup.
                    if doc.get("name").is_none() {
                        if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                            doc["name"] = Value::String(stem.to_string());
                        }
                    }
                    playbooks.push(doc);
                } else {
                    errors.push(format!("{}: not a mapping", path.display()));
                }
            }
            Err(e) => errors.push(format!("{}: {e}", path.display())),
        }
    }

    playbooks.sort_by(|a, b| {
        a["name"]
            .as_str()
            .unwrap_or("")
            .cmp(b["name"].as_str().unwrap_or(""))
    });
    (playbooks, errors)
}

/// `GET /playbooks` — all playbook documents; unparsable files are logged
/// instead of being dropped silently.
pub async fn list_playbooks() -> Result<Json<Value>, (StatusCode, String)> {
    let (playbooks, errors) = load_playbooks();
    if !errors.is_empty() {
        warn!(?errors, "playbook parse errors");
    }
    Ok(Json(Value::Array(playbooks)))
}

/// `GET /playbooks/:name` — one playbook by `name` or filename stem.
pub async fn get_playbook(Path(name): Path<String>) -> Result<Json<Value>, (StatusCode, String)> {
    let (playbooks, errors) = load_playbooks();
    if !errors.is_empty() {
        warn!(?errors, "playbook parse errors");
    }
    playbooks
        .into_iter()
        .find(|p| p["name"].as_str() == Some(name.as_str()))
        .map(Json)
        .ok_or((StatusCode::NOT_FOUND, format!("playbook {name} not found")))
}
