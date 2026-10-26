//! A2A agent card: the discovery document served from
//! `/.well-known/agent.json`.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Capabilities an agent advertises in its [`AgentCard`].
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Capabilities {
    /// Whether the agent supports SSE via `tasks/sendSubscribe`.
    pub streaming: bool,
    /// Whether the agent supports push-notification callbacks.
    #[serde(rename = "pushNotifications")]
    pub push_notifications: bool,
    /// Whether the agent exposes prior task status transitions.
    #[serde(rename = "stateTransitionHistory")]
    pub state_transition_history: bool,
}

/// Authentication requirements declared by an [`AgentCard`].
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Authentication {
    /// Accepted scheme names (e.g. `bearer`).
    pub schemes: Vec<String>,
}

/// One skill advertised by an [`AgentCard`].
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Skill {
    /// Stable skill identifier.
    pub id: String,
    /// Human-readable skill name.
    pub name: String,
    /// What the skill does.
    pub description: String,
    /// Categorization tags.
    #[serde(default)]
    pub tags: Vec<String>,
    /// Example prompts or invocations.
    #[serde(default)]
    pub examples: Vec<String>,
}

/// The A2A discovery document for one agent.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AgentCard {
    /// Agent name.
    pub name: String,
    /// What the agent does.
    pub description: String,
    /// JSON-RPC endpoint clients post tasks to.
    pub url: String,
    /// Agent implementation version.
    pub version: String,
    /// Content types the agent accepts (`text`, `data`).
    #[serde(rename = "defaultInputModes")]
    pub default_input_modes: Vec<String>,
    /// Content types the agent returns.
    #[serde(rename = "defaultOutputModes")]
    pub default_output_modes: Vec<String>,
    /// Supported A2A features.
    pub capabilities: Capabilities,
    /// Required authentication.
    pub authentication: Authentication,
    /// Skills the agent advertises.
    pub skills: Vec<Skill>,
}
