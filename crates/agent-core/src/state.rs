//! Agent run state: the ordered list of steps and token accounting.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use agent_protocol::tool::ToolCall;

/// One step in an agent run transcript.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Step {
    /// A reasoning or planning note emitted by the model.
    Plan {
        /// The plan text.
        text: String,
    },
    /// A tool call issued by the agent.
    ToolCall {
        /// The call as dispatched to the gateway.
        call: ToolCall,
    },
    /// An observation fed back to the model (tool output or parse error).
    Observation {
        /// The observation text.
        text: String,
    },
    /// The agent's final answer; terminates the run.
    Final {
        /// The final answer text.
        text: String,
    },
}

/// Checkpointable state of a running agent thread.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentState {
    /// Identifier of the thread this state belongs to.
    pub thread_id: Uuid,
    /// The original input the run started from.
    pub input: String,
    /// Ordered transcript of steps taken so far.
    pub steps: Vec<Step>,
    /// Total tokens (input + output) consumed so far.
    pub tokens_used: u64,
}
