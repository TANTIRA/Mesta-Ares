//! `/controls/kill-switch` handlers.
//!
//! The kill switch state is defined by `agent_core::kill_switch_engaged()`:
//! `ARES_KILL_SWITCH` env or a non-empty `$POLICIES_DIR/kill` file (default
//! `policies/kill`). POST writes or removes that file.

use std::path::PathBuf;

use agent_core::kill_switch_engaged;
use axum::{body::Bytes, http::StatusCode, response::IntoResponse, Json};
use serde::Deserialize;
use serde_json::json;

/// Optional POST body: `{"engaged": bool}` — defaults to engage when absent.
#[derive(Debug, Deserialize)]
struct KillBody {
    /// Desired state; `true` engages the switch.
    #[serde(default = "default_true")]
    engaged: bool,
}

fn default_true() -> bool {
    true
}

/// Resolves the kill-file path (`$POLICIES_DIR/kill`, default `policies/kill`).
fn kill_file() -> PathBuf {
    agent_protocol::kill::policies_dir().join(agent_protocol::kill::KILL_FILE)
}

/// `GET /controls/kill-switch` — current switch state.
pub async fn get_state() -> impl IntoResponse {
    Json(json!({"engaged": kill_switch_engaged()}))
}

/// `POST /controls/kill-switch` — engage (`{"engaged":true}`, the default for
/// an empty body) or disengage (`{"engaged":false}`) the switch.
pub async fn set_state(body: Bytes) -> impl IntoResponse {
    let engaged = if body.is_empty() {
        true
    } else {
        serde_json::from_slice::<KillBody>(&body)
            .map(|b| b.engaged)
            .unwrap_or(true)
    };
    let path = kill_file();
    let result = if engaged {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        std::fs::write(&path, "engaged\n").map(|_| "engaged")
    } else {
        match std::fs::remove_file(&path) {
            Ok(()) => Ok("disengaged"),
            Err(e) if !path.exists() => {
                let _ = e; // already absent is a successful disengage
                Ok("disengaged")
            }
            Err(e) => Err(e),
        }
    };
    match result {
        Ok(state) => (
            StatusCode::OK,
            Json(json!({"state": state, "engaged": kill_switch_engaged()})),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": e.to_string(), "path": path.display().to_string()})),
        ),
    }
}
