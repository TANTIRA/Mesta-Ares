//! Tool invocation contract between agents, the policy engine, and the
//! tool server.

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A request to invoke a tool through the tool gateway.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ToolCall {
    /// Unique identifier of this call; echoed back in [`ToolResult::call_id`].
    pub id: Uuid,
    /// Name of the tool to invoke.
    pub tool: String,
    /// Tool arguments; the schema is defined per tool.
    pub args: serde_json::Value,
    /// Identity of the actor (agent, service, or human) requesting the call.
    pub actor: String,
    /// Idempotency key; replays of the same key must not re-execute effects.
    pub idempotency_key: String,
    /// Correlation identifier tying the call back to its triggering flow.
    pub correlation_id: Uuid,
}

/// The outcome of a policy decision on a [`ToolCall`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DecisionKind {
    /// The call may execute immediately.
    Allow,
    /// The call is forbidden.
    Deny,
    /// The call requires human approval before execution.
    ApprovalRequired,
}

/// A policy decision rendered on a tool call.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Decision {
    /// The kind of decision rendered.
    pub kind: DecisionKind,
    /// Human-readable justification for the decision.
    pub reason: String,
    /// Timestamp at which the decision was made.
    pub decided_at: DateTime<Utc>,
}

/// The result of an invoked tool call.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ToolResult {
    /// Identifier of the [`ToolCall`] this result answers.
    pub call_id: Uuid,
    /// Whether the call completed successfully.
    pub ok: bool,
    /// Tool output on success; schema is defined per tool.
    pub output: serde_json::Value,
    /// Error description when `ok` is `false`.
    pub error: Option<String>,
}

/// `output` field name carrying the retryable flag. The tool server sets it
/// when a failure is transient (claim in flight, store unavailable) so the
/// gateway retries instead of treating the call as terminal.
const RETRYABLE_FIELD: &str = "retryable";

impl ToolResult {
    /// `true` when the tool server flagged this result as safe to retry.
    pub fn is_retryable(&self) -> bool {
        self.output
            .get(RETRYABLE_FIELD)
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false)
    }

    /// A failed result flagged retryable: `ok=false`, `error` set, and
    /// `output` carries `{ "retryable": true }`.
    pub fn retryable(call_id: Uuid, error: impl Into<String>) -> Self {
        let mut output = serde_json::Map::new();
        output.insert(RETRYABLE_FIELD.to_string(), serde_json::Value::Bool(true));
        Self {
            call_id,
            ok: false,
            output: serde_json::Value::Object(output),
            error: Some(error.into()),
        }
    }
}
