//! `GET /playbooks` and `GET /playbooks/:name` — response playbooks on disk.
//!
//! Scans `PLAYBOOKS_DIR` (default `playbooks/`) for `*.yml`/`*.yaml` files
//! and returns each parsed document. A playbook is a graph document:
//! `name`, `description`, `trigger`, `nodes` (each with `id`, `kind`,
//! optional `label`/`tool`/`args`) and `edges` (`{from, to}`).

use axum::{extract::Path, http::StatusCode, Json};
use serde_json::Value;
use tracing::warn;

/// Loads every playbook document in `PLAYBOOKS_DIR`, sorted by name.
fn load_playbooks() -> (Vec<Value>, Vec<String>) {
    let dir = agent_protocol::paths::dir_from_env("PLAYBOOKS_DIR", "playbooks");
    let (docs, errors) = crate::yaml_dir::load(&dir);
    let mut playbooks: Vec<Value> = Vec::new();
    let mut errors = errors;
    for (stem, mut doc) in docs {
        if !doc.is_object() {
            errors.push(format!("{stem}: not a mapping"));
            continue;
        }
        if doc.get("name").is_none() {
            doc["name"] = Value::String(stem);
        }
        playbooks.push(doc);
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
