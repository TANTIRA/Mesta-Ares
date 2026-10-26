//! Handoff payload used when one agent delegates work to another.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Context an agent passes to a receiving agent when handing off a task.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct HandoffPayload {
    /// The objective the receiving agent should pursue.
    pub goal: String,
    /// Constraints the receiving agent must respect (scope, budget, time).
    pub constraints: Vec<String>,
    /// Artifacts produced so far that the receiver should use.
    pub artifacts: Vec<Artifact>,
    /// Questions the handing-off agent could not answer.
    pub open_questions: Vec<String>,
}

/// A reference to an artifact produced during an agent run.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Artifact {
    /// Category of the artifact (e.g. `report`, `query`, `evidence`).
    pub kind: String,
    /// URI where the artifact content can be retrieved.
    pub ref_uri: String,
    /// Short human-readable summary of the artifact.
    pub summary: String,
}
