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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminal_steps_close_only_for_known_agents() {
        assert!(terminal_step("triage", "close"));
        assert!(terminal_step("response", "done"));
        assert!(terminal_step("detection", "rejected"));
        assert!(terminal_step("hunt", "done"));
        assert!(!terminal_step("hunt", "run"));
        assert!(!terminal_step("unknown-agent", "done"));
    }

    #[test]
    fn aborted_steps_are_halts() {
        assert!(aborted_step("kill_switch"));
        assert!(aborted_step("rejected"));
        assert!(!aborted_step("done"));
        assert!(!aborted_step("close"));
    }

    #[test]
    fn event_round_trips_through_json() {
        let event = ActivityEvent::new(Uuid::nil(), "triage", "close", "fp recorded");
        let text = serde_json::to_string(&event).expect("serialize");
        let back: ActivityEvent = serde_json::from_str(&text).expect("deserialize");
        assert_eq!(back.agent, "triage");
        assert_eq!(back.step, "close");
        assert_eq!(back.workflow_run_id, Uuid::nil());
    }
}
