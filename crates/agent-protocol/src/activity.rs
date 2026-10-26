//! Agent activity events published to NATS `ares.activity`.
//!
//! One shape for every agent, so the console SSE feed and the A2A/ACP run
//! streams filter by `workflow_run_id` uniformly.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// NATS subject carrying agent activity events.
pub const SUBJECT_ACTIVITY: &str = "ares.activity";

/// One step of agent progress visible to the console.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivityEvent {
    /// Correlation id tying the event to its originating envelope/case.
    pub workflow_run_id: Uuid,
    /// Name of the emitting agent (e.g. `triage`).
    pub agent: String,
    /// Short step identifier (e.g. `enrich`, `verdict`).
    pub step: String,
    /// Free-form detail about the step.
    pub detail: String,
    /// Emission timestamp.
    pub ts: DateTime<Utc>,
}

/// Activity steps that mean the agent's work on a run is finished, keyed by
/// `ActivityEvent.agent`.
pub fn terminal_step(agent: &str, step: &str) -> bool {
    match agent {
        "triage" => matches!(step, "close" | "handoff" | "kill_switch"),
        "response" => matches!(step, "done" | "kill_switch"),
        "detection" => matches!(step, "deployed" | "rejected" | "kill_switch"),
        "hunt" => matches!(step, "done" | "kill_switch"),
        _ => false,
    }
}

/// Terminal steps that end a run as a halt rather than a success.
pub fn aborted_step(step: &str) -> bool {
    matches!(step, "kill_switch" | "rejected")
}

impl ActivityEvent {
    /// Build an event for `workflow_run_id` from `agent` at step `step`.
    pub fn new(
        workflow_run_id: Uuid,
        agent: impl Into<String>,
        step: impl Into<String>,
        detail: impl Into<String>,
    ) -> Self {
        Self {
            workflow_run_id,
            agent: agent.into(),
            step: step.into(),
            detail: detail.into(),
            ts: Utc::now(),
        }
    }
}
