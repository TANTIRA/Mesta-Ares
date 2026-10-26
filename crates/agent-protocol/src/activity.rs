//! Agent activity events published to NATS `ares.activity`.
//!
//! One shape for every agent, so the console SSE feed and the A2A/ACP run
//! streams filter by `workflow_run_id` uniformly.

use chrono::{DateTime, Utc};
use serde::Serialize;
use uuid::Uuid;

/// NATS subject carrying agent activity events.
pub const SUBJECT_ACTIVITY: &str = "ares.activity";

/// One step of agent progress visible to the console.
#[derive(Debug, Clone, Serialize)]
pub struct ActivityEvent {
    /// Correlation id tying the event to its originating envelope/case.
    pub workflow_run_id: Uuid,
    /// Name of the emitting agent (e.g. `triage`).
    pub agent: &'static str,
    /// Short step identifier (e.g. `enrich`, `verdict`).
    pub step: String,
    /// Free-form detail about the step.
    pub detail: String,
    /// Emission timestamp.
    pub ts: DateTime<Utc>,
}

impl ActivityEvent {
    /// Build an event for `workflow_run_id` from `agent` at step `step`.
    pub fn new(
        workflow_run_id: Uuid,
        agent: &'static str,
        step: impl Into<String>,
        detail: impl Into<String>,
    ) -> Self {
        Self {
            workflow_run_id,
            agent,
            step: step.into(),
            detail: detail.into(),
            ts: Utc::now(),
        }
    }
}
