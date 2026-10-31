//! `/controls/kill-switch` handlers.
//!
//! The switch state is defined by [`agent_protocol::kill::kill_switch_engaged`]:
//! the `ARES_KILL_SWITCH` env override, or a non-empty kill file in the shared
//! control directory (`$ARES_CONTROL_DIR/kill`, default `$POLICIES_DIR/kill`).
//!
//! In every shipped deployment `ARES_CONTROL_DIR` is a volume mounted
//! read-write here and read-only by tool-server and every agent, so the state
//! written by `POST` is the state the policy gate actually observes. `POST`
//! verifies the write and returns 500 when it did not land somewhere the gate
//! can see, instead of reporting a success that has no effect.

use std::path::Path;
use std::sync::Arc;

use agent_core::kill_switch_engaged;
use agent_protocol::kill::{kill_file, kill_file_engaged_path, kill_switch_source};
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

/// Writes (`engaged = true`) or removes the kill file at `path`.
///
/// The parent directory is created when needed so a fresh control volume works
/// on first engage.
fn apply_state(path: &Path, engaged: bool) -> std::io::Result<&'static str> {
    if engaged {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }
        std::fs::write(path, "engaged\n")?;
        Ok("engaged")
    } else {
        match std::fs::remove_file(path) {
            Ok(()) => Ok("disengaged"),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                let _ = e; // already absent is a successful disengage
                Ok("disengaged")
            }
            Err(e) => Err(e),
        }
    }
}

/// `GET /controls/kill-switch` — current switch state and its source.
pub async fn get_state() -> impl IntoResponse {
    let source = kill_switch_source();
    Json(json!({
        "engaged": kill_switch_engaged(),
        "source": source.as_str(),
    }))
}

/// `POST /controls/kill-switch` — engage (`{"engaged":true}`, the default for
/// an empty body) or disengage (`{"engaged":false}`) the switch. The actor
/// comes from the authenticated request and the change is audited.
///
/// Returns 500 when the state the gate reads did not change: a write that only
/// lands in this container's filesystem is not a working kill switch.
pub async fn set_state(
    State(s): State<Arc<AppState>>,
    Extension(actor): Extension<http_limits::auth::Actor>,
    body: Bytes,
) -> Result<impl IntoResponse, crate::error::ApiError> {
    let engaged = if body.is_empty() {
        true
    } else {
        serde_json::from_slice::<KillBody>(&body)
            .map(|b| b.engaged)
            .unwrap_or(true)
    };
    let path = kill_file();
    let result = apply_state(&path, engaged);

    let actor = actor.name;
    match result {
        Ok(state) => {
            // Verify the state the enforcement points will read. This is the
            // check that turns "POST returned 200" into "the kill switch
            // actually engaged".
            if kill_file_engaged_path(&path) != engaged {
                return Err(crate::error::ApiError::internal(
                    "kill-switch write did not take effect; \
                     check that ARES_CONTROL_DIR is a shared writable mount",
                )
                .with_detail("file", path.display().to_string()));
            }
            if let Some(writer) = &s.audit {
                if let Err(e) = writer
                    .append(
                        &actor,
                        "controls.kill_switch",
                        &json!({"engaged": engaged, "file": path.display().to_string()}),
                        DecisionKind::Allow,
                        &format!("kill switch {state} by {actor} (file {})", path.display()),
                        Uuid::new_v4(),
                    )
                    .await
                {
                    tracing::error!(error = %e, "kill-switch audit append failed");
                }
            }
            Ok((
                StatusCode::OK,
                Json(json!({
                    "state": state,
                    "engaged": kill_switch_engaged(),
                    "source": kill_switch_source().as_str(),
                    "file": path.display().to_string(),
                })),
            ))
        }
        Err(e) => Err(crate::error::ApiError::internal(e.to_string())
            .with_detail("file", path.display().to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn temp_path(tag: &str) -> PathBuf {
        std::env::temp_dir()
            .join(format!("ares-kill-{}-{tag}", Uuid::new_v4()))
            .join("kill")
    }

    #[test]
    fn apply_state_writes_and_removes_the_kill_file() {
        let path = temp_path("apply");
        assert!(!kill_file_engaged_path(&path), "absent file disengages");

        assert_eq!(apply_state(&path, true).unwrap(), "engaged");
        assert!(kill_file_engaged_path(&path), "write engages the gate");

        assert_eq!(apply_state(&path, false).unwrap(), "disengaged");
        assert!(!kill_file_engaged_path(&path), "remove disengages");

        // Removing an already absent file is a successful disengage.
        assert_eq!(apply_state(&path, false).unwrap(), "disengaged");

        std::fs::remove_dir_all(path.parent().unwrap()).ok();
    }

    #[test]
    fn apply_state_creates_the_control_directory() {
        let path = temp_path("nested");
        assert!(!path.parent().unwrap().exists());
        apply_state(&path, true).unwrap();
        assert!(path.exists());
        std::fs::remove_dir_all(path.parent().unwrap()).ok();
    }
}
