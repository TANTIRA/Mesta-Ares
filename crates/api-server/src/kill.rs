//! `/controls/kill-switch` handlers.
//!
//! The kill switch state is defined by `agent_core::kill_switch_engaged()`:
//! `ARES_KILL_SWITCH` env or a non-empty `$POLICIES_DIR/kill` file (default
//! `policies/kill`). POST writes or removes that file.

use std::path::PathBuf;

use std::sync::Arc;

use agent_core::kill_switch_engaged;
use agent_protocol::tool::DecisionKind;
use axum::{
    body::Bytes,
    extract::{Extension, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

use crate::AppState;

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
/// an empty body) or disengage (`{"engaged":false}`) the switch. The actor
/// comes from the authenticated request and the change is audited.
pub async fn set_state(
    State(s): State<Arc<AppState>>,
    Extension(actor): Extension<http_limits::auth::Actor>,
    body: Bytes,
) -> impl IntoResponse {
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
    let actor = actor.0;
    match result {
        Ok(state) => {
            if let Some(writer) = &s.audit {
                if let Err(e) = writer
                    .append(
                        &actor,
                        "controls.kill_switch",
                        &json!({"engaged": engaged}),
                        DecisionKind::Allow,
                        &format!("kill switch {state} by {actor}"),
                        Uuid::new_v4(),
                    )
                    .await
                {
                    tracing::error!(error = %e, "kill-switch audit append failed");
                }
            }
            (
                StatusCode::OK,
                Json(json!({"state": state, "engaged": kill_switch_engaged()})),
            )
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": e.to_string(), "path": path.display().to_string()})),
        ),
    }
}
