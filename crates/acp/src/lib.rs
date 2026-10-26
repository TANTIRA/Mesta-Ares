//! ACP (Agent Communication Protocol) types for ARES.
//!
//! Serde models for the ACP v0.2.0 REST contract: agent manifests
//! (`GET /agents`), run lifecycle (`POST /runs`, `GET /runs/{id}`,
//! `POST /runs/{id}`, `POST /runs/{id}/cancel`), sessions
//! (`GET /session/{id}`) and the structured [`Error`] body. Transport and
//! persistence live in the embedding service.

#![warn(missing_docs)]

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// ACP error codes (OpenAPI `Error.code` enum).
pub const ERR_SERVER: &str = "server_error";
/// See [`ERR_SERVER`].
pub const ERR_INVALID_INPUT: &str = "invalid_input";
/// See [`ERR_SERVER`].
pub const ERR_NOT_FOUND: &str = "not_found";

/// Structured ACP error body.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Error {
    /// Machine-readable code (`server_error`, `invalid_input`, `not_found`).
    pub code: String,
    /// Human-readable detail.
    pub message: String,
    /// Optional structured payload.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

impl Error {
    /// Builds an [`Error`] with no `data`.
    pub fn new(code: &str, message: impl Into<String>) -> Self {
        Self {
            code: code.to_string(),
            message: message.into(),
            data: None,
        }
    }
}

/// One part of an ACP [`Message`]: inline `content` or a `content_url`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct MessagePart {
    /// Optional artifact name for this part.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// MIME type of the content (`text/plain` when absent on the wire).
    #[serde(default = "default_content_type")]
    pub content_type: String,
    /// Inline content; mutually exclusive with `content_url`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    /// `plain` or `base64`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_encoding: Option<String>,
    /// URL the part content lives at.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_url: Option<String>,
    /// Citation or trajectory metadata. Never emit private reasoning here.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Value>,
}

fn default_content_type() -> String {
    "text/plain".to_string()
}

/// An ACP message: `user`, `agent`, or `agent/{name}`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Message {
    /// Sender role: `user`, `agent`, `agent/{agent_name}`.
    pub role: String,
    /// Ordered message parts.
    pub parts: Vec<MessagePart>,
    /// Creation time.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<DateTime<Utc>>,
    /// Completion time (for streamed messages).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<DateTime<Utc>>,
}

impl Message {
    /// Builds a single `text/plain` part message from `agent/{name}`.
    pub fn agent_text(agent_name: &str, text: impl Into<String>) -> Self {
        Self {
            role: format!("agent/{agent_name}"),
            parts: vec![MessagePart {
                name: None,
                content_type: "text/plain".to_string(),
                content: Some(text.into()),
                content_encoding: None,
                content_url: None,
                metadata: None,
            }],
            created_at: Some(Utc::now()),
            completed_at: Some(Utc::now()),
        }
    }

    /// Joins all inline text parts into one string.
    pub fn text(&self) -> String {
        self.parts
            .iter()
            .filter_map(|p| p.content.as_deref())
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// ACP agent manifest served by `GET /agents` and `GET /agents/{name}`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AgentManifest {
    /// RFC 1123 DNS-label agent identifier.
    pub name: String,
    /// Human-readable description.
    pub description: String,
    /// MIME types accepted in input messages.
    pub input_content_types: Vec<String>,
    /// MIME types produced in output messages.
    pub output_content_types: Vec<String>,
    /// Free-form manifest metadata (capabilities, domains, tags, links).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Value>,
}

/// ACP run status values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum RunStatus {
    /// Run accepted, not yet started.
    Created,
    /// Agent is executing.
    InProgress,
    /// Run waits on client input.
    Awaiting,
    /// Cancellation requested.
    Cancelling,
    /// Run was cancelled.
    Cancelled,
    /// Run finished successfully.
    Completed,
    /// Run failed.
    Failed,
}

impl RunStatus {
    /// `true` for `cancelled`, `completed`, `failed`.
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Cancelled | Self::Completed | Self::Failed)
    }

    /// Wire spelling of the status.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Created => "created",
            Self::InProgress => "in-progress",
            Self::Awaiting => "awaiting",
            Self::Cancelling => "cancelling",
            Self::Cancelled => "cancelled",
            Self::Completed => "completed",
            Self::Failed => "failed",
        }
    }

    /// Parses a stored status string; unknown values map to `InProgress`.
    pub fn parse(s: &str) -> Self {
        match s {
            "created" => Self::Created,
            "awaiting" => Self::Awaiting,
            "cancelling" => Self::Cancelling,
            "cancelled" => Self::Cancelled,
            "completed" => Self::Completed,
            "failed" => Self::Failed,
            _ => Self::InProgress,
        }
    }
}

/// `POST /runs` execution mode.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum RunMode {
    /// Block until the run reaches a terminal state.
    Sync,
    /// Return the created run immediately.
    #[default]
    Async,
    /// Return an `text/event-stream` of run events.
    Stream,
}

/// ACP run object.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Run {
    /// Agent executing the run.
    pub agent_name: String,
    /// Optional session grouping.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    /// Run UUID.
    pub run_id: String,
    /// Current status.
    pub status: RunStatus,
    /// What the run awaits, when `status == awaiting`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub await_request: Option<Value>,
    /// Output messages produced so far.
    pub output: Vec<Message>,
    /// Failure detail, when `status == failed`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<Error>,
    /// Creation time.
    pub created_at: DateTime<Utc>,
    /// Terminal time.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finished_at: Option<DateTime<Utc>>,
}

/// `POST /runs` body.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct RunCreateRequest {
    /// Agent to run.
    pub agent_name: String,
    /// Existing session to attach the run to.
    #[serde(default)]
    pub session_id: Option<String>,
    /// Session object carrying `id`, `history` and `state` URLs.
    #[serde(default)]
    pub session: Option<Session>,
    /// Input messages (min 1 on the wire).
    pub input: Vec<Message>,
    /// Execution mode; `async` when absent.
    #[serde(default)]
    pub mode: RunMode,
}

/// `POST /runs/{run_id}` body (resume an awaiting run).
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct RunResumeRequest {
    /// Run being resumed.
    pub run_id: String,
    /// Client payload answering the await request.
    pub await_resume: Value,
    /// Execution mode for the resumed run.
    pub mode: RunMode,
}

/// ACP session: history and state are URL references.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Session {
    /// Session UUID.
    pub id: String,
    /// URLs of session history entries.
    pub history: Vec<String>,
    /// URL of the session state document.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_status_kebab_case_round_trips() {
        let s = serde_json::to_string(&RunStatus::InProgress).unwrap();
        assert_eq!(s, "\"in-progress\"");
        assert_eq!(
            serde_json::from_str::<RunStatus>("\"cancelled\"").unwrap(),
            RunStatus::Cancelled
        );
        assert!(RunStatus::Completed.is_terminal());
        assert!(!RunStatus::InProgress.is_terminal());
    }

    #[test]
    fn message_text_joins_inline_parts() {
        let mut m = Message::agent_text("triage-agent", "hello");
        m.parts.push(MessagePart {
            name: None,
            content_type: "text/plain".to_string(),
            content: Some("world".to_string()),
            content_encoding: None,
            content_url: None,
            metadata: None,
        });
        assert_eq!(m.text(), "hello\nworld");
        assert_eq!(m.role, "agent/triage-agent");
    }

    #[test]
    fn create_request_defaults_to_async() {
        let req: RunCreateRequest = serde_json::from_value(serde_json::json!({
            "agent_name": "triage-agent",
            "input": [{"role": "user", "parts": [{"content_type": "text/plain", "content": "hi"}]}]
        }))
        .unwrap();
        assert_eq!(req.mode, RunMode::Async);
    }
}
