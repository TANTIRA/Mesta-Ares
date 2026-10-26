//! JSON-RPC 2.0 envelope and the A2A method dispatcher.
//!
//! The dispatcher is transport- and storage-agnostic: callers supply a
//! [`TaskBackend`] that owns task persistence and agent invocation.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::task::{SendParams, Task, TaskIdParams};

/// JSON-RPC method: submit a task and receive its initial state.
pub const METHOD_SEND: &str = "tasks/send";
/// JSON-RPC method: fetch a task's current state.
pub const METHOD_GET: &str = "tasks/get";
/// JSON-RPC method: request cancellation of a task.
pub const METHOD_CANCEL: &str = "tasks/cancel";
/// JSON-RPC method: like `tasks/send`, followed by an SSE subscription.
pub const METHOD_SEND_SUBSCRIBE: &str = "tasks/sendSubscribe";

/// Error code: the request payload is not valid JSON.
pub const PARSE_ERROR: i64 = -32700;
/// Error code: the payload is not a well-formed JSON-RPC request.
pub const INVALID_REQUEST: i64 = -32600;
/// Error code: `method` is not one of the supported A2A methods.
pub const METHOD_NOT_FOUND: i64 = -32601;
/// Error code: `params` failed validation.
pub const INVALID_PARAMS: i64 = -32602;
/// Error code: an internal backend failure.
pub const INTERNAL_ERROR: i64 = -32000;
/// Error code: no task exists for the requested id.
pub const TASK_NOT_FOUND: i64 = -32001;

/// A JSON-RPC 2.0 request.
#[derive(Debug, Deserialize)]
pub struct JsonRpcRequest {
    /// The JSON-RPC version string (`"2.0"`).
    pub jsonrpc: String,
    /// Client-chosen request id, echoed in the response.
    #[serde(default)]
    pub id: Value,
    /// Method name, e.g. `tasks/send`.
    pub method: String,
    /// Method parameters.
    #[serde(default)]
    pub params: Value,
}

/// A JSON-RPC 2.0 error object.
#[derive(Debug, Serialize)]
pub struct RpcError {
    /// Numeric error code.
    pub code: i64,
    /// Short human-readable description.
    pub message: String,
    /// Optional structured detail.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

/// A JSON-RPC 2.0 response; exactly one of `result`/`error` is set.
#[derive(Debug, Serialize)]
pub struct JsonRpcResponse {
    /// Always `"2.0"`.
    pub jsonrpc: &'static str,
    /// Echoed request id (`null` for parse failures).
    pub id: Value,
    /// Method result on success.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    /// Error object on failure.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<RpcError>,
}

impl JsonRpcResponse {
    /// Success response echoing `id` with `result`.
    pub fn ok(id: Value, result: Value) -> Self {
        Self {
            jsonrpc: "2.0",
            id,
            result: Some(result),
            error: None,
        }
    }

    /// Error response echoing `id`.
    pub fn err(id: Value, code: i64, message: impl Into<String>) -> Self {
        Self {
            jsonrpc: "2.0",
            id,
            result: None,
            error: Some(RpcError {
                code,
                message: message.into(),
                data: None,
            }),
        }
    }
}

/// Backend-side failure surfaced as a JSON-RPC error.
#[derive(Debug)]
pub enum BackendError {
    /// The parameters were structurally valid JSON but not acceptable input
    /// (maps to [`INVALID_PARAMS`]).
    InvalidParams(String),
    /// Any other failure (maps to [`INTERNAL_ERROR`]).
    Internal(String),
}

impl BackendError {
    fn code(&self) -> i64 {
        match self {
            BackendError::InvalidParams(_) => INVALID_PARAMS,
            BackendError::Internal(_) => INTERNAL_ERROR,
        }
    }
}

impl std::fmt::Display for BackendError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BackendError::InvalidParams(m) | BackendError::Internal(m) => f.write_str(m),
        }
    }
}

impl std::error::Error for BackendError {}

/// Storage + invocation hooks the dispatcher calls per method.
///
/// Implemented by the embedding service (api-server), which owns the
/// `a2a_tasks` table and the NATS handoff to agents.
#[async_trait::async_trait]
pub trait TaskBackend: Send + Sync {
    /// Create a task from `tasks/send`/`tasks/sendSubscribe` params and start
    /// the underlying work. Returns the task in its initial state.
    async fn send(
        &self,
        agent: &str,
        params: SendParams,
        subscribe: bool,
    ) -> Result<Task, BackendError>;

    /// Return the current state of `task_id`, or `None` when unknown.
    async fn get(&self, task_id: &str) -> Result<Option<Task>, BackendError>;

    /// Cancel `task_id`; returns the canceled task, or `None` when unknown.
    async fn cancel(&self, task_id: &str) -> Result<Option<Task>, BackendError>;
}

/// Routes one [`JsonRpcRequest`] to `backend` for the agent named `agent`.
///
/// `agent` is the URL path segment the request was addressed to; the backend
/// validates that it is a known agent. Method results serialize as the A2A
/// `Task` object.
pub async fn dispatch<B: TaskBackend + ?Sized>(
    backend: &B,
    agent: &str,
    req: JsonRpcRequest,
) -> JsonRpcResponse {
    let id = req.id;
    if req.jsonrpc != "2.0" {
        return JsonRpcResponse::err(
            id,
            INVALID_REQUEST,
            format!("unsupported jsonrpc version: {}", req.jsonrpc),
        );
    }
    match req.method.as_str() {
        METHOD_SEND | METHOD_SEND_SUBSCRIBE => {
            let params: SendParams = match serde_json::from_value(req.params) {
                Ok(p) => p,
                Err(e) => {
                    return JsonRpcResponse::err(id, INVALID_PARAMS, format!("invalid params: {e}"))
                }
            };
            match backend
                .send(agent, params, req.method == METHOD_SEND_SUBSCRIBE)
                .await
            {
                Ok(task) => match serde_json::to_value(&task) {
                    Ok(v) => JsonRpcResponse::ok(id, v),
                    Err(e) => JsonRpcResponse::err(id, INTERNAL_ERROR, e.to_string()),
                },
                Err(e) => JsonRpcResponse::err(id, e.code(), e.to_string()),
            }
        }
        METHOD_GET | METHOD_CANCEL => {
            let params: TaskIdParams = match serde_json::from_value(req.params) {
                Ok(p) => p,
                Err(e) => {
                    return JsonRpcResponse::err(id, INVALID_PARAMS, format!("invalid params: {e}"))
                }
            };
            let result = if req.method == METHOD_GET {
                backend.get(&params.id).await
            } else {
                backend.cancel(&params.id).await
            };
            match result {
                Ok(Some(task)) => match serde_json::to_value(&task) {
                    Ok(v) => JsonRpcResponse::ok(id, v),
                    Err(e) => JsonRpcResponse::err(id, INTERNAL_ERROR, e.to_string()),
                },
                Ok(None) => JsonRpcResponse::err(
                    id,
                    TASK_NOT_FOUND,
                    format!("task not found: {}", params.id),
                ),
                Err(e) => JsonRpcResponse::err(id, e.code(), e.to_string()),
            }
        }
        other => JsonRpcResponse::err(id, METHOD_NOT_FOUND, format!("method not found: {other}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::task::{Message, Task, TaskState, TaskStatus};
    use serde_json::json;

    /// In-memory stub backend: `send` echoes a working task, `get` knows
    /// only `"exists"`, `cancel` returns canceled.
    struct Stub;

    #[async_trait::async_trait]
    impl TaskBackend for Stub {
        async fn send(
            &self,
            agent: &str,
            params: SendParams,
            _subscribe: bool,
        ) -> Result<Task, BackendError> {
            if agent != "stub-agent" {
                return Err(BackendError::InvalidParams("unknown agent".into()));
            }
            Ok(Task {
                id: params.id.unwrap_or_else(|| "t-1".into()),
                session_id: params.session_id,
                status: TaskStatus::now(TaskState::Working),
                artifacts: None,
                history: Some(vec![params.message]),
                metadata: None,
            })
        }

        async fn get(&self, task_id: &str) -> Result<Option<Task>, BackendError> {
            Ok((task_id == "exists").then(|| Task {
                id: task_id.into(),
                session_id: None,
                status: TaskStatus::now(TaskState::Working),
                artifacts: None,
                history: None,
                metadata: None,
            }))
        }

        async fn cancel(&self, task_id: &str) -> Result<Option<Task>, BackendError> {
            Ok((task_id == "exists").then(|| Task {
                id: task_id.into(),
                session_id: None,
                status: TaskStatus::now(TaskState::Canceled),
                artifacts: None,
                history: None,
                metadata: None,
            }))
        }
    }

    fn req(method: &str, params: Value) -> JsonRpcRequest {
        JsonRpcRequest {
            jsonrpc: "2.0".into(),
            id: json!("r-1"),
            method: method.into(),
            params,
        }
    }

    #[tokio::test]
    async fn dispatch_send_returns_task() {
        let resp = dispatch(
            &Stub,
            "stub-agent",
            req(
                METHOD_SEND,
                json!({"id": "t-9", "message": {"role": "user", "parts": [{"kind": "text", "text": "hi"}]}}),
            ),
        )
        .await;
        assert!(resp.error.is_none());
        assert_eq!(resp.result.unwrap()["status"]["state"], "working");
    }

    #[tokio::test]
    async fn dispatch_unknown_method_and_params() {
        let resp = dispatch(&Stub, "stub-agent", req("bogus/method", json!({}))).await;
        assert_eq!(resp.error.unwrap().code, METHOD_NOT_FOUND);

        let resp = dispatch(&Stub, "stub-agent", req(METHOD_SEND, json!({"id": 1}))).await;
        assert_eq!(resp.error.unwrap().code, INVALID_PARAMS);
    }

    #[tokio::test]
    async fn dispatch_get_missing_task() {
        let resp = dispatch(&Stub, "stub-agent", req(METHOD_GET, json!({"id": "nope"}))).await;
        assert_eq!(resp.error.unwrap().code, TASK_NOT_FOUND);
    }

    #[tokio::test]
    async fn dispatch_send_unknown_agent_is_invalid_params() {
        let resp = dispatch(
            &Stub,
            "ghost",
            req(
                METHOD_SEND,
                json!({"message": {"role": "user", "parts": [{"kind": "text", "text": "hi"}]}}),
            ),
        )
        .await;
        assert_eq!(resp.error.unwrap().code, INVALID_PARAMS);
    }

    #[test]
    fn message_text_joins_text_parts() {
        let m = Message {
            role: crate::task::Role::User,
            parts: vec![
                crate::task::Part::Text { text: "a".into() },
                crate::task::Part::Data {
                    data: json!({"x": 1}),
                },
                crate::task::Part::Text { text: "b".into() },
            ],
            metadata: None,
        };
        assert_eq!(m.text().as_deref(), Some("a\nb"));
    }
}
