//! A2A (Agent2Agent) protocol endpoints.
//!
//! Routes:
//! - `GET  /.well-known/agent.json`                — platform card (Ares orchestrator).
//! - `GET  /a2a/agents`                            — all invocable agent cards.
//! - `GET  /a2a/{agent}/.well-known/agent.json`    — one agent's card.
//! - `POST /a2a/{agent}`                           — JSON-RPC: `tasks/send`,
//!   `tasks/get`, `tasks/cancel`, `tasks/sendSubscribe`.
//! - `GET  /a2a/tasks/{id}/events`                 — per-task SSE stream.
//!
//! `tasks/send` wraps the A2A message in an [`Envelope`] whose kind routes to
//! the agent's NATS subject (triage→`Alert`, response→`Command`,
//! detection→`Intel`, hunt→`HuntFinding`) and publishes it on that subject
//! directly; the orchestrator skips envelopes already at their destination.
//! `correlation_id` = the task's run UUID, so `ares.activity` events for the
//! run join back to the task by id.

use std::convert::Infallible;
use std::sync::Arc;

use a2a::card::{AgentCard, Authentication, Capabilities, Skill};
use a2a::rpc::{self, BackendError, JsonRpcResponse, TaskBackend};
use a2a::task::{Message, Part, Role, SendParams, Task, TaskState, TaskStatus};
use agent_protocol::envelope::Envelope;
use agent_protocol::handoff::HandoffPayload;
use agent_protocol::SUBJECT_RESPOND;
use axum::{
    body::Bytes,
    extract::{Path, State},
    http::StatusCode,
    response::sse::{Event, Sse},
    response::IntoResponse,
    Json,
};
use chrono::{DateTime, Utc};
use futures::Stream;
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;

use crate::agents::{find_agent, AgentDef, AGENTS};
use crate::run_stream::{self, CheckpointProgress, RunRecord};
use crate::AppState;

/// Authentication schemes advertised on cards: bearer only when inbound
/// authentication is actually configured.
fn auth_schemes(auth_enabled: bool) -> Authentication {
    Authentication {
        schemes: if auth_enabled {
            vec!["bearer".to_string()]
        } else {
            Vec::new()
        },
    }
}

/// Builds the [`AgentCard`] for one agent.
fn card_for(def: &AgentDef, auth_enabled: bool) -> AgentCard {
    AgentCard {
        name: def.name.to_string(),
        description: def.description.to_string(),
        url: format!("/a2a/{}", def.name),
        version: env!("CARGO_PKG_VERSION").to_string(),
        default_input_modes: vec!["text".to_string(), "data".to_string()],
        default_output_modes: vec!["text".to_string()],
        capabilities: Capabilities {
            streaming: true,
            push_notifications: false,
            state_transition_history: false,
        },
        authentication: auth_schemes(auth_enabled),
        skills: def
            .skills
            .iter()
            .map(|(id, name, description, tags, examples)| Skill {
                id: id.to_string(),
                name: name.to_string(),
                description: description.to_string(),
                tags: tags.iter().map(|t| t.to_string()).collect(),
                examples: examples.iter().map(|e| e.to_string()).collect(),
            })
            .collect(),
    }
}

/// Top-level discovery card representing the Ares platform itself.
fn orchestrator_card(auth_enabled: bool) -> AgentCard {
    AgentCard {
        name: "ares-orchestrator".to_string(),
        description: "Ares agentic SIEM+SOAR: routes work to the triage, response, detection and hunt agents.".to_string(),
        url: "/a2a".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        default_input_modes: vec!["text".to_string(), "data".to_string()],
        default_output_modes: vec!["text".to_string()],
        capabilities: Capabilities {
            streaming: true,
            push_notifications: false,
            state_transition_history: false,
        },
        authentication: auth_schemes(auth_enabled),
        skills: AGENTS
            .iter()
            .map(|def| Skill {
                id: def.name.to_string(),
                name: def.name.to_string(),
                description: def.description.to_string(),
                tags: vec!["agent".to_string()],
                examples: vec![format!("POST /a2a/{} tasks/send", def.name)],
            })
            .collect(),
    }
}

/// Creates the `a2a_tasks` table (and `agent_checkpoints`, owned by
/// agent-core, so the task-state join cannot fail on a fresh database).
pub async fn ensure_schema(pool: &PgPool) -> anyhow::Result<()> {
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS a2a_tasks (
            id TEXT PRIMARY KEY,
            agent TEXT NOT NULL,
            session_id TEXT,
            state TEXT NOT NULL,
            message JSONB,
            created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
            updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
        )",
    )
    .execute(pool)
    .await?;
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS agent_checkpoints (
            thread_id UUID PRIMARY KEY,
            state JSONB NOT NULL,
            updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
        )",
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// Derives the workflow-run UUID for a task id: the id itself when it is a
/// UUID, else a deterministic v5 hash so `tasks/get`/SSE can still join
/// `agent_checkpoints` and `ares.activity` by correlation id.
pub(crate) fn task_uuid(task_id: &str) -> Uuid {
    Uuid::parse_str(task_id)
        .unwrap_or_else(|_| Uuid::new_v5(&Uuid::NAMESPACE_URL, task_id.as_bytes()))
}

/// A stored `a2a_tasks` row.
#[derive(Debug, sqlx::FromRow)]
struct TaskRow {
    /// Task id (client-supplied or generated).
    id: String,
    /// Agent the task was sent to.
    agent: String,
    /// Optional session grouping.
    session_id: Option<String>,
    /// Stored spec-spelled state.
    state: String,
    /// The original user message.
    message: Option<sqlx::types::Json<Message>>,
    /// Last mutation time; doubles as the status timestamp.
    updated_at: DateTime<Utc>,
}

/// Loads a task row, folding in `agent_checkpoints` progress when the stored
/// state is not terminal: a checkpoint whose last step is `final` means the
/// run completed. Returns the task and the stored agent name.
async fn load_task(pool: &PgPool, task_id: &str) -> Result<Option<(Task, String)>, BackendError> {
    let row = sqlx::query_as::<_, TaskRow>(
        "SELECT id, agent, session_id, state, message, updated_at FROM a2a_tasks WHERE id = $1",
    )
    .bind(task_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| BackendError::Internal(e.to_string()))?;
    let Some(row) = row else { return Ok(None) };

    let mut state = TaskState::parse(&row.state).unwrap_or(TaskState::Submitted);
    let mut status_message = None;

    if !state.is_terminal() {
        match run_stream::checkpoint_progress(pool, &row.id)
            .await
            .map_err(|e| BackendError::Internal(e.to_string()))?
        {
            CheckpointProgress::Final(text) => {
                state = TaskState::Completed;
                status_message = Some(Message {
                    role: Role::Agent,
                    parts: vec![Part::Text { text }],
                    metadata: None,
                });
                Task::mark_terminal(pool, &row.id, false).await;
            }
            CheckpointProgress::InProgress => state = TaskState::Working,
            CheckpointProgress::None => {}
        }
    }

    Ok(Some((
        Task {
            id: row.id,
            session_id: row.session_id,
            status: TaskStatus {
                state,
                message: status_message,
                timestamp: row.updated_at,
            },
            artifacts: None,
            history: row.message.map(|m| vec![m.0]),
            metadata: None,
        },
        row.agent,
    )))
}

/// Builds the [`Envelope`] payload for the target agent.
///
/// response-agent consumes a [`HandoffPayload`], so the A2A message becomes
/// its `goal`. Other agents read the payload as free-form context.
pub(crate) fn build_payload(
    def: &AgentDef,
    task_id: &str,
    msg: &Message,
    meta: &Option<Value>,
) -> Value {
    if def.subject == SUBJECT_RESPOND {
        let goal = msg
            .text()
            .unwrap_or_else(|| serde_json::to_string(msg).unwrap_or_default());
        return serde_json::to_value(HandoffPayload {
            goal: goal.clone(),
            constraints: vec![],
            artifacts: vec![],
            open_questions: vec![],
        })
        .unwrap_or_else(|_| json!({"goal": goal}));
    }
    json!({
        "a2a_task_id": task_id,
        "message": msg,
        "metadata": meta,
    })
}

/// Postgres + NATS implementation of the A2A task backend.
struct PgBackend<'a> {
    /// Shared API-server state.
    s: &'a AppState,
}

#[async_trait::async_trait]
impl TaskBackend for PgBackend<'_> {
    async fn send(
        &self,
        agent: &str,
        params: SendParams,
        subscribe: bool,
    ) -> Result<Task, BackendError> {
        let def = find_agent(agent)
            .ok_or_else(|| BackendError::InvalidParams(format!("unknown agent: {agent}")))?;
        let task_id = params
            .id
            .clone()
            .unwrap_or_else(|| Uuid::new_v4().to_string());

        let inserted = sqlx::query(
            "INSERT INTO a2a_tasks (id, agent, session_id, state, message, created_at, updated_at)
             VALUES ($1, $2, $3, $5, $4, now(), now())
             ON CONFLICT (id) DO NOTHING",
        )
        .bind(&task_id)
        .bind(def.name)
        .bind(&params.session_id)
        .bind(sqlx::types::Json(&params.message))
        .bind(TaskState::Working.as_str())
        .execute(&self.s.pool)
        .await
        .map_err(|e| BackendError::Internal(e.to_string()))?;
        if inserted.rows_affected() == 0 {
            return Err(BackendError::InvalidParams(format!(
                "task id already exists: {task_id}"
            )));
        }

        let mut env = Envelope::new(
            format!("a2a:{task_id}"),
            def.kind,
            build_payload(def, &task_id, &params.message, &params.metadata),
        );
        env.correlation_id = task_uuid(&task_id);
        if let Err(e) = self.s.sink.publish(def.subject, &env).await {
            if let Err(werr) =
                sqlx::query("UPDATE a2a_tasks SET state = $2, updated_at = now() WHERE id = $1")
                    .bind(&task_id)
                    .bind(TaskState::Failed.as_str())
                    .execute(&self.s.pool)
                    .await
            {
                tracing::error!(task_id = %task_id, error = %werr, "failed-state write failed");
            }
            return Err(BackendError::Internal(format!(
                "publish {}: {e}",
                def.subject
            )));
        }

        // tasks/sendSubscribe advertises where the SSE stream lives by
        // merging an `sse` key into the task metadata.
        let metadata = if subscribe {
            let mut m = params.metadata.unwrap_or_else(|| json!({}));
            if let Some(obj) = m.as_object_mut() {
                obj.insert(
                    "sse".to_string(),
                    json!(format!("/a2a/tasks/{task_id}/events")),
                );
            }
            Some(m)
        } else {
            params.metadata
        };

        Ok(Task {
            id: task_id.clone(),
            session_id: params.session_id.clone(),
            status: TaskStatus::now(TaskState::Working),
            artifacts: None,
            history: Some(vec![params.message]),
            metadata,
        })
    }

    async fn get(&self, agent: &str, task_id: &str) -> Result<Option<Task>, BackendError> {
        Ok(load_task(&self.s.pool, task_id)
            .await?
            .filter(|(_, stored_agent)| stored_agent == agent)
            .map(|(t, _)| t))
    }

    async fn cancel(&self, agent: &str, task_id: &str) -> Result<Option<Task>, BackendError> {
        // Only marks the row; in-flight agents do not observe the flag.
        // (known gap: agents never poll a cancellation table.)
        sqlx::query(
            "UPDATE a2a_tasks SET state = $3, updated_at = now()
             WHERE id = $1 AND agent = $2 AND state NOT IN ($4, $5, $6)",
        )
        .bind(task_id)
        .bind(agent)
        .bind(TaskState::Canceled.as_str())
        .bind(TaskState::Completed.as_str())
        .bind(TaskState::Failed.as_str())
        .bind(TaskState::Canceled.as_str())
        .execute(&self.s.pool)
        .await
        .map_err(|e| BackendError::Internal(e.to_string()))?;
        self.get(agent, task_id).await
    }
}

/// `GET /a2a/agents` — every invocable agent card.
pub async fn list_agents(State(s): State<Arc<AppState>>) -> Json<Value> {
    Json(json!(AGENTS
        .iter()
        .map(|def| card_for(def, s.auth_enabled))
        .collect::<Vec<_>>()))
}

/// `GET /.well-known/agent.json` — the platform-level card.
pub async fn well_known_card(State(s): State<Arc<AppState>>) -> Json<AgentCard> {
    Json(orchestrator_card(s.auth_enabled))
}

/// `GET /a2a/{agent}/.well-known/agent.json` — one agent's card.
pub async fn agent_card(
    State(s): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> Result<Json<AgentCard>, StatusCode> {
    find_agent(&name)
        .map(|def| Json(card_for(def, s.auth_enabled)))
        .ok_or(StatusCode::NOT_FOUND)
}

/// `POST /a2a/{agent}` — JSON-RPC 2.0 endpoint for A2A task methods.
pub async fn rpc_endpoint(
    State(s): State<Arc<AppState>>,
    Path(agent): Path<String>,
    body: Bytes,
) -> Json<JsonRpcResponse> {
    let req: rpc::JsonRpcRequest = match serde_json::from_slice(&body) {
        Ok(r) => r,
        Err(e) => {
            return Json(JsonRpcResponse::err(
                Value::Null,
                rpc::PARSE_ERROR,
                format!("parse error: {e}"),
            ))
        }
    };
    let backend = PgBackend { s: &s };
    Json(rpc::dispatch(&backend, &agent, req).await)
}

#[async_trait::async_trait]
impl RunRecord for Task {
    fn is_terminal(&self) -> bool {
        self.status.state.is_terminal()
    }

    fn record_event(&self) -> Event {
        Event::default()
            .event("task")
            .data(serde_json::to_string(self).unwrap_or_default())
    }

    fn activity_event(activity: Value) -> Event {
        Event::default()
            .event("activity")
            .data(activity.to_string())
    }

    async fn load(pool: &PgPool, id: &str) -> Result<Option<Self>, String> {
        load_task(pool, id)
            .await
            .map(|found| found.map(|(task, _)| task))
            .map_err(|e| e.to_string())
    }

    async fn mark_terminal(pool: &PgPool, id: &str, aborted: bool) {
        let state = if aborted {
            TaskState::Failed
        } else {
            TaskState::Completed
        };
        if let Err(e) =
            sqlx::query("UPDATE a2a_tasks SET state = $2, updated_at = now() WHERE id = $1")
                .bind(id)
                .bind(state.as_str())
                .execute(pool)
                .await
        {
            tracing::error!(task_id = %id, error = %e, "terminal state write failed");
        }
    }
}

/// `GET /a2a/tasks/{id}/events` — SSE for one task: replays the stored task,
/// then streams `ares.activity` events for its workflow run, and closes with
/// a final `task` event when the agent reports a terminal step.
pub async fn task_events(
    State(s): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, impl IntoResponse> {
    let (task, activity_name) = match load_task(&s.pool, &id).await {
        Ok(Some((t, agent))) => (
            t,
            find_agent(&agent)
                .map(|d| d.activity_name)
                .unwrap_or_default(),
        ),
        Ok(None) => return Err((StatusCode::NOT_FOUND, format!("task not found: {id}"))),
        Err(e) => return Err((StatusCode::INTERNAL_SERVER_ERROR, e.to_string())),
    };

    Ok(run_stream::activity_stream::<Task>(
        s.activity_tx.subscribe(),
        s.pool.clone(),
        id.clone(),
        task_uuid(&id).to_string(),
        activity_name.to_string(),
        task,
    ))
}

// Task rows and SSE filtering use `serde_json::Value` traversal; typed
// request deserialization lives in the `a2a` crate.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_uuid_is_stable_for_non_uuid_ids() {
        let a = task_uuid("not-a-uuid");
        let b = task_uuid("not-a-uuid");
        assert_eq!(a, b);
        assert_ne!(a, task_uuid("other"));
        let real = Uuid::new_v4();
        assert_eq!(task_uuid(&real.to_string()), real);
    }

    #[test]
    fn response_tasks_receive_a_handoff_goal() {
        let def = find_agent("response-agent").expect("agent");
        let msg = Message::user_text("contain the host");
        let payload = build_payload(def, "task-1", &msg, &None);
        assert_eq!(payload["goal"], "contain the host");
        assert!(payload["constraints"].is_array());
    }

    #[test]
    fn other_agents_receive_the_raw_message() {
        let def = find_agent("triage-agent").expect("agent");
        let msg = Message::user_text("alert");
        let payload = build_payload(def, "task-1", &msg, &None);
        assert_eq!(payload["a2a_task_id"], "task-1");
        assert!(payload["message"].is_object());
    }
}
