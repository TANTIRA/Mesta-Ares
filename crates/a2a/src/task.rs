//! A2A task and message types.
//!
//! Mirrors the A2A spec's `Task`, `TaskStatus`, `Message` and `Part` shapes.
//! Field names serialize to the spec's JSON spelling (`sessionId`, `kind`).

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Lifecycle state of an A2A [`Task`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum TaskState {
    /// The task was received but work has not started.
    #[serde(rename = "submitted")]
    Submitted,
    /// The task is actively being worked.
    #[serde(rename = "working")]
    Working,
    /// The task is paused awaiting additional user input.
    #[serde(rename = "input-required")]
    InputRequired,
    /// The task finished successfully.
    #[serde(rename = "completed")]
    Completed,
    /// The task terminated with an error.
    #[serde(rename = "failed")]
    Failed,
    /// The task was canceled before completion.
    #[serde(rename = "canceled")]
    Canceled,
}

impl TaskState {
    /// `true` for states no running task can leave: completed, failed, canceled.
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            TaskState::Completed | TaskState::Failed | TaskState::Canceled
        )
    }

    /// Lowercase spec spelling of the state (e.g. `input-required`).
    pub fn as_str(self) -> &'static str {
        match self {
            TaskState::Submitted => "submitted",
            TaskState::Working => "working",
            TaskState::InputRequired => "input-required",
            TaskState::Completed => "completed",
            TaskState::Failed => "failed",
            TaskState::Canceled => "canceled",
        }
    }

    /// Parses a stored spec spelling back into a [`TaskState`].
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "submitted" => TaskState::Submitted,
            "working" => TaskState::Working,
            "input-required" => TaskState::InputRequired,
            "completed" => TaskState::Completed,
            "failed" => TaskState::Failed,
            "canceled" => TaskState::Canceled,
            _ => return None,
        })
    }
}

/// Sender role of an A2A [`Message`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    /// The remote client driving the task.
    User,
    /// The agent answering the task.
    Agent,
}

/// A single content part of a [`Message`] or [`Artifact`].
///
/// Serialized with the spec's `kind` discriminator (`text` or `data`).
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Part {
    /// Plain-text content.
    Text {
        /// The text payload.
        text: String,
    },
    /// Structured JSON content.
    Data {
        /// Arbitrary structured payload.
        data: Value,
    },
}

/// A message exchanged between client and agent.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Message {
    /// Who authored the message.
    pub role: Role,
    /// Ordered content parts.
    pub parts: Vec<Part>,
    /// Optional message metadata.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Value>,
}

impl Message {
    /// Builds a single-text-part user message.
    pub fn user_text(text: impl Into<String>) -> Self {
        Self {
            role: Role::User,
            parts: vec![Part::Text { text: text.into() }],
            metadata: None,
        }
    }

    /// Concatenated text of all `Text` parts; `None` when there are none.
    pub fn text(&self) -> Option<String> {
        let texts: Vec<&str> = self
            .parts
            .iter()
            .filter_map(|p| match p {
                Part::Text { text } => Some(text.as_str()),
                Part::Data { .. } => None,
            })
            .collect();
        if texts.is_empty() {
            None
        } else {
            Some(texts.join("\n"))
        }
    }
}

/// A unit of agent output attached to a [`Task`].
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Artifact {
    /// Client-visible artifact identifier.
    #[serde(rename = "artifactId")]
    pub artifact_id: String,
    /// Optional human-readable name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// The artifact content.
    pub parts: Vec<Part>,
}

/// Point-in-time status of a [`Task`].
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct TaskStatus {
    /// Current lifecycle state.
    pub state: TaskState,
    /// Optional message attached to this status (e.g. a final answer).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<Message>,
    /// When this status was recorded.
    pub timestamp: DateTime<Utc>,
}

impl TaskStatus {
    /// Builds a status for `state` stamped at the current time.
    pub fn now(state: TaskState) -> Self {
        Self {
            state,
            message: None,
            timestamp: Utc::now(),
        }
    }
}

/// An A2A task: a unit of work addressed to an agent.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Task {
    /// Client-supplied or server-generated task identifier.
    pub id: String,
    /// Session grouping identifier; tasks sharing it share context.
    #[serde(rename = "sessionId", default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    /// Current status.
    pub status: TaskStatus,
    /// Outputs produced by the agent, when any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifacts: Option<Vec<Artifact>>,
    /// Message history for the task, oldest first.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub history: Option<Vec<Message>>,
    /// Free-form task metadata.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Value>,
}

/// Parameters of the `tasks/send` and `tasks/sendSubscribe` methods.
#[derive(Debug, Clone, Deserialize)]
pub struct SendParams {
    /// Client-chosen task id; a UUID is generated server-side when absent.
    #[serde(default)]
    pub id: Option<String>,
    /// Optional session the task belongs to.
    #[serde(rename = "sessionId", default)]
    pub session_id: Option<String>,
    /// The user message describing the work.
    pub message: Message,
    /// Optional request metadata, echoed into the task.
    #[serde(default)]
    pub metadata: Option<Value>,
}

/// Parameters of the `tasks/get` and `tasks/cancel` methods.
#[derive(Debug, Clone, Deserialize)]
pub struct TaskIdParams {
    /// Identifier of the task to fetch or cancel.
    pub id: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn part_serializes_with_kind_tag() {
        let p = Part::Text {
            text: "hi".to_string(),
        };
        assert_eq!(
            serde_json::to_value(&p).unwrap(),
            serde_json::json!({"kind": "text", "text": "hi"})
        );
    }

    #[test]
    fn task_round_trips() {
        let task = Task {
            id: "t-1".to_string(),
            session_id: Some("s-1".to_string()),
            status: TaskStatus::now(TaskState::Working),
            artifacts: None,
            history: Some(vec![Message::user_text("investigate")]),
            metadata: None,
        };
        let v = serde_json::to_value(&task).unwrap();
        assert_eq!(v["sessionId"], "s-1");
        assert_eq!(v["status"]["state"], "working");
        let back: Task = serde_json::from_value(v).unwrap();
        assert_eq!(back.id, "t-1");
    }

    #[test]
    fn task_state_terminal() {
        assert!(TaskState::Completed.is_terminal());
        assert!(TaskState::Failed.is_terminal());
        assert!(TaskState::Canceled.is_terminal());
        assert!(!TaskState::Working.is_terminal());
    }
}
