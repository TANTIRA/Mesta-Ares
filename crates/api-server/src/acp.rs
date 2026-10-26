//! ACP (Agent Communication Protocol) endpoints.
//!
//! Routes (mounted under `/acp`):
//! - `GET  /acp/ping`                      — liveness.
//! - `GET  /acp/agents`                    — `{agents: [AgentManifest]}`.
//! - `GET  /acp/agents/{name}`             — one manifest.
//! - `GET  /.well-known/agent.yml`         — platform manifest (YAML).
//! - `POST /acp/runs`                      — run create; `mode` selects
//!   `sync` (200), `async` (202) or `stream` (SSE).
//! - `GET  /acp/runs/{run_id}`             — run status/output.
//! - `POST /acp/runs/{run_id}`             — resume an awaiting run.
//! - `POST /acp/runs/{run_id}/cancel`      — cancel (202).
//! - `GET  /acp/runs/{run_id}/events`      — per-run SSE stream.
//! - `GET  /acp/session/{session_id}`      — session history URLs.
//!
//! Runs reuse the [`crate::a2a`] agent registry and NATS publish path: the
//! run UUID is the envelope `correlation_id`, so `ares.activity` events and
//! `agent_checkpoints` join back to the run by id.

use std::convert::Infallible;
use std::sync::Arc;
use std::time::Duration;

use acp::{
    AgentManifest, Error as AcpError, Message, Run, RunCreateRequest, RunMode, RunResumeRequest,
    RunStatus, Session, ERR_INVALID_INPUT, ERR_NOT_FOUND, ERR_SERVER,
};
use agent_protocol::envelope::Envelope;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::sse::{Event, Sse},
    response::IntoResponse,
    Json,
};
use chrono::{DateTime, Utc};
use futures::Stream;
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::PgPool;
use tokio::sync::broadcast;
use uuid::Uuid;

use crate::a2a;
use crate::agents;
use crate::run_stream::{self, CheckpointProgress, RunRecord};
use crate::AppState;

/// Max wait for `mode: "sync"` runs before returning the live status.
const SYNC_TIMEOUT: Duration = Duration::from_secs(120);

/// `(StatusCode, Json<acp::Error>)` shorthand.
fn acp_err(
    status: StatusCode,
    code: &str,
    message: impl Into<String>,
) -> (StatusCode, Json<AcpError>) {
    (status, Json(AcpError::new(code, message)))
}

/// Builds the [`AgentManifest`] for one Ares agent.
fn manifest_for(def: &agents::AgentDef) -> AgentManifest {
    AgentManifest {
        name: def.name.to_string(),
        description: def.description.to_string(),
        input_content_types: vec!["text/plain".to_string(), "application/json".to_string()],
        output_content_types: vec!["text/plain".to_string()],
        metadata: Some(json!({
            "protocol_version": "0.2.0",
            "domains": ["security-operations"],
            "capabilities": def
                .skills
                .iter()
                .map(|(_, name, description, _, _)| json!({"name": name, "description": description}))
                .collect::<Vec<_>>(),
        })),
    }
}

/// Creates the `acp_runs` table.
pub async fn ensure_schema(pool: &PgPool) -> anyhow::Result<()> {
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS acp_runs (
            id TEXT PRIMARY KEY,
            agent TEXT NOT NULL,
            session_id TEXT,
            status TEXT NOT NULL,
            input JSONB,
            output JSONB,
            error JSONB,
            created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
            finished_at TIMESTAMPTZ,
            updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
        )",
    )
    .execute(pool)
    .await?;
    sqlx::query("CREATE INDEX IF NOT EXISTS acp_runs_session_idx ON acp_runs (session_id)")
        .execute(pool)
        .await?;
    Ok(())
}

/// A stored `acp_runs` row.
#[derive(Debug, sqlx::FromRow)]
struct RunRow {
    /// Run UUID string.
    id: String,
    /// Agent name.
    agent: String,
    /// Optional session id.
    session_id: Option<String>,
    /// Spec-spelled status.
    status: String,
    /// Persisted output messages.
    output: Option<sqlx::types::Json<Vec<Message>>>,
    /// Persisted error.
    error: Option<sqlx::types::Json<AcpError>>,
    /// Creation time.
    created_at: DateTime<Utc>,
    /// Terminal time.
    finished_at: Option<DateTime<Utc>>,
}

impl RunRow {
    /// Builds the wire [`Run`] from the row.
    fn into_run(self, status: RunStatus, output: Vec<Message>) -> Run {
        Run {
            agent_name: self.agent,
            session_id: self.session_id,
            run_id: self.id,
            status,
            await_request: None,
            output,
            error: self.error.map(|e| e.0),
            created_at: self.created_at,
            finished_at: self.finished_at,
        }
    }
}

/// Loads a run, folding in `agent_checkpoints` progress when the stored
/// status is not terminal: a checkpoint whose last step is `final` means the
/// run completed and its text becomes the output message. Persists the
/// write-back.
async fn load_run(pool: &PgPool, run_id: &str) -> Result<Option<Run>, AcpError> {
    let row = sqlx::query_as::<_, RunRow>(
        "SELECT id, agent, session_id, status, output, error, created_at, finished_at
         FROM acp_runs WHERE id = $1",
    )
    .bind(run_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| AcpError::new(ERR_SERVER, e.to_string()))?;
    let Some(row) = row else { return Ok(None) };

    let mut status = RunStatus::parse(&row.status);
    let mut output = row.output.as_ref().map(|o| o.0.clone()).unwrap_or_default();

    if !status.is_terminal() {
        match run_stream::checkpoint_progress(pool, &row.id)
            .await
            .map_err(|e| AcpError::new(ERR_SERVER, e.to_string()))?
        {
            CheckpointProgress::Final(text) => {
                status = RunStatus::Completed;
                output.push(Message::agent_text(&row.agent, text));
                if let Err(e) = sqlx::query(
                    "UPDATE acp_runs SET status = $3, output = $2,
                     finished_at = now(), updated_at = now() WHERE id = $1",
                )
                .bind(&row.id)
                .bind(sqlx::types::Json(&output))
                .bind(RunStatus::Completed.as_str())
                .execute(pool)
                .await
                {
                    tracing::error!(run_id = %row.id, error = %e, "completion write failed");
                }
            }
            CheckpointProgress::InProgress | CheckpointProgress::None => {}
        }
    }

    Ok(Some(row.into_run(status, output)))
}

/// Waits for a terminal `ares.activity` step for `run_id`, then reloads the
/// run. Times out after [`SYNC_TIMEOUT`], returning the run as it stands.
async fn wait_terminal(
    rx: broadcast::Receiver<String>,
    pool: &PgPool,
    run_id: &str,
    activity_agent: &str,
) -> Result<Run, AcpError> {
    run_stream::wait_for_terminal(
        rx,
        &a2a::task_uuid(run_id).to_string(),
        activity_agent,
        SYNC_TIMEOUT,
    )
    .await;
    load_run(pool, run_id)
        .await?
        .ok_or_else(|| AcpError::new(ERR_NOT_FOUND, format!("run not found: {run_id}")))
}

/// Joins ACP input messages into one text blob for the agent envelope.
fn input_to_text(input: &[Message]) -> String {
    let text = input
        .iter()
        .map(Message::text)
        .filter(|t| !t.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n");
    if text.is_empty() {
        serde_json::to_string(input).unwrap_or_default()
    } else {
        text
    }
}

/// Inserts the run row and publishes the agent envelope. On publish failure
/// the row is marked `failed`.
async fn dispatch_run(
    s: &AppState,
    def: &agents::AgentDef,
    run_id: &str,
    session_id: Option<&str>,
    input: &[Message],
) -> Result<(), AcpError> {
    let msg = ::a2a::task::Message::user_text(input_to_text(input));
    let meta = Some(json!({"acp_run_id": run_id, "input": input}));

    sqlx::query(
        "INSERT INTO acp_runs (id, agent, session_id, status, input, created_at, updated_at)
         VALUES ($1, $2, $3, $5, $4, now(), now())",
    )
    .bind(run_id)
    .bind(def.name)
    .bind(session_id)
    .bind(sqlx::types::Json(input))
    .bind(RunStatus::InProgress.as_str())
    .execute(&s.pool)
    .await
    .map_err(|e| AcpError::new(ERR_SERVER, e.to_string()))?;

    let mut env = Envelope::new(
        format!("acp:{run_id}"),
        def.kind,
        a2a::build_payload(def, run_id, &msg, &meta),
    );
    env.correlation_id = a2a::task_uuid(run_id);
    if let Err(e) = s.sink.publish(def.subject, &env).await {
        if let Err(werr) = sqlx::query(
            "UPDATE acp_runs SET status = $3, error = $2, finished_at = now(),
             updated_at = now() WHERE id = $1",
        )
        .bind(run_id)
        .bind(sqlx::types::Json(AcpError::new(
            ERR_SERVER,
            format!("publish {}: {e}", def.subject),
        )))
        .bind(RunStatus::Failed.as_str())
        .execute(&s.pool)
        .await
        {
            tracing::error!(run_id = %run_id, error = %werr, "failed-status write failed");
        }
        return Err(AcpError::new(
            ERR_SERVER,
            format!("publish {}: {e}", def.subject),
        ));
    }
    Ok(())
}

/// `GET /acp/ping`.
pub async fn ping() -> Json<Value> {
    Json(json!({}))
}

/// Query params for `GET /acp/agents`.
#[derive(Debug, Deserialize)]
pub struct AgentsQuery {
    /// Max manifests to return.
    limit: Option<usize>,
    /// Offset into the list.
    offset: Option<usize>,
}

/// `GET /acp/agents`.
pub async fn list_agents(Query(q): Query<AgentsQuery>) -> Json<Value> {
    let offset = q.offset.unwrap_or(0);
    let agents: Vec<_> = agents::AGENTS
        .iter()
        .skip(offset)
        .take(q.limit.unwrap_or(usize::MAX))
        .map(manifest_for)
        .collect();
    Json(json!({"agents": agents}))
}

/// `GET /acp/agents/{name}`.
pub async fn get_agent(
    Path(name): Path<String>,
) -> Result<Json<AgentManifest>, (StatusCode, Json<AcpError>)> {
    agents::find_agent(&name)
        .map(|def| Json(manifest_for(def)))
        .ok_or_else(|| {
            acp_err(
                StatusCode::NOT_FOUND,
                ERR_NOT_FOUND,
                format!("agent not found: {name}"),
            )
        })
}

/// `GET /.well-known/agent.yml` — platform manifest in YAML.
pub async fn well_known_manifest() -> impl IntoResponse {
    let manifest = AgentManifest {
        name: "ares-orchestrator".to_string(),
        description:
            "Ares agentic SIEM+SOAR: ACP front for the triage, response, detection and hunt agents."
                .to_string(),
        input_content_types: vec!["text/plain".to_string(), "application/json".to_string()],
        output_content_types: vec!["text/plain".to_string()],
        metadata: Some(json!({
            "protocol_version": "0.2.0",
            "agents": agents::AGENTS.iter().map(|d| d.name).collect::<Vec<_>>(),
        })),
    };
    (
        [(axum::http::header::CONTENT_TYPE, "application/yaml")],
        serde_yaml_ng::to_string(&manifest).unwrap_or_default(),
    )
}

/// `POST /acp/runs` — dispatch follows `mode` (`async` default).
pub async fn create_run(
    State(s): State<Arc<AppState>>,
    Json(req): Json<RunCreateRequest>,
) -> Result<impl IntoResponse, (StatusCode, Json<AcpError>)> {
    let def = agents::find_agent(&req.agent_name).ok_or_else(|| {
        acp_err(
            StatusCode::NOT_FOUND,
            ERR_NOT_FOUND,
            format!("agent not found: {}", req.agent_name),
        )
    })?;
    if req.input.is_empty() {
        return Err(acp_err(
            StatusCode::BAD_REQUEST,
            ERR_INVALID_INPUT,
            "input must contain at least one message",
        ));
    }
    let run_id = Uuid::new_v4().to_string();
    let session_id = req
        .session_id
        .or_else(|| req.session.as_ref().map(|s| s.id.clone()));

    // Subscribe before publish so terminal activity cannot be missed.
    let rx = s.activity_tx.subscribe();
    dispatch_run(&s, def, &run_id, session_id.as_deref(), &req.input)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(e)))?;

    let load = || async {
        load_run(&s.pool, &run_id)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(e)))?
            .ok_or_else(|| {
                acp_err(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    ERR_SERVER,
                    "run vanished after create",
                )
            })
    };
    match req.mode {
        RunMode::Async => Ok((StatusCode::ACCEPTED, Json(load().await?)).into_response()),
        RunMode::Sync => {
            let run = wait_terminal(rx, &s.pool, &run_id, def.activity_name)
                .await
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(e)))?;
            Ok((StatusCode::OK, Json(run)).into_response())
        }
        RunMode::Stream => {
            let run = load().await?;
            Ok(run_stream::activity_stream::<Run>(
                rx,
                s.pool.clone(),
                run.run_id.clone(),
                a2a::task_uuid(&run.run_id).to_string(),
                def.activity_name.to_string(),
                run,
            )
            .into_response())
        }
    }
}

/// `GET /acp/runs/{run_id}`.
pub async fn get_run(
    State(s): State<Arc<AppState>>,
    Path(run_id): Path<String>,
) -> Result<Json<Run>, (StatusCode, Json<AcpError>)> {
    load_run(&s.pool, &run_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(e)))?
        .map(Json)
        .ok_or_else(|| {
            acp_err(
                StatusCode::NOT_FOUND,
                ERR_NOT_FOUND,
                format!("run not found: {run_id}"),
            )
        })
}

/// `POST /acp/runs/{run_id}` — resume. Ares agents never emit `awaiting`, so
/// this always rejects with `invalid_input`; kept for spec completeness.
pub async fn resume_run(
    State(s): State<Arc<AppState>>,
    Path(run_id): Path<String>,
    Json(_req): Json<RunResumeRequest>,
) -> Result<Json<Run>, (StatusCode, Json<AcpError>)> {
    let run = load_run(&s.pool, &run_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(e)))?
        .ok_or_else(|| {
            acp_err(
                StatusCode::NOT_FOUND,
                ERR_NOT_FOUND,
                format!("run not found: {run_id}"),
            )
        })?;
    Err(acp_err(
        StatusCode::BAD_REQUEST,
        ERR_INVALID_INPUT,
        format!("run {run_id} is '{}', not awaiting", run.status.as_str()),
    ))
}

/// `POST /acp/runs/{run_id}/cancel` — 202 with the run. Only marks the row;
/// in-flight agents do not observe the flag (same gap as A2A cancel).
pub async fn cancel_run(
    State(s): State<Arc<AppState>>,
    Path(run_id): Path<String>,
) -> Result<impl IntoResponse, (StatusCode, Json<AcpError>)> {
    sqlx::query(
        "UPDATE acp_runs SET status = $2, finished_at = now(), updated_at = now()
         WHERE id = $1 AND status NOT IN ($3, $4, $5)",
    )
    .bind(&run_id)
    .bind(RunStatus::Cancelled.as_str())
    .bind(RunStatus::Completed.as_str())
    .bind(RunStatus::Failed.as_str())
    .bind(RunStatus::Cancelled.as_str())
    .execute(&s.pool)
    .await
    .map_err(|e| acp_err(StatusCode::INTERNAL_SERVER_ERROR, ERR_SERVER, e.to_string()))?;
    // Absent row → 404; already-terminal row → returned as-is.
    load_run(&s.pool, &run_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(e)))?
        .map(|r| (StatusCode::ACCEPTED, Json(r)).into_response())
        .ok_or_else(|| {
            acp_err(
                StatusCode::NOT_FOUND,
                ERR_NOT_FOUND,
                format!("run not found: {run_id}"),
            )
        })
}

/// Serializes a run-status SSE event (`{"type": "run.<status>", "run": …}`).
fn run_event(run: &Run) -> Event {
    Event::default().event("run").data(
        serde_json::to_string(&json!({
            "type": format!("run.{}", run.status.as_str()),
            "run": run,
        }))
        .unwrap_or_default(),
    )
}

#[async_trait::async_trait]
impl RunRecord for Run {
    fn is_terminal(&self) -> bool {
        self.status.is_terminal()
    }

    fn record_event(&self) -> Event {
        run_event(self)
    }

    fn activity_event(activity: Value) -> Event {
        Event::default().event("run").data(
            serde_json::to_string(&json!({"type": "generic", "generic": activity}))
                .unwrap_or_default(),
        )
    }

    async fn load(pool: &PgPool, id: &str) -> Result<Option<Self>, String> {
        load_run(pool, id).await.map_err(|e| e.message)
    }

    async fn mark_terminal(pool: &PgPool, id: &str, aborted: bool) {
        let status = if aborted {
            RunStatus::Failed
        } else {
            RunStatus::Completed
        };
        if let Err(e) = sqlx::query(
            "UPDATE acp_runs SET status = $2, finished_at = now(), updated_at = now() WHERE id = $1",
        )
        .bind(id)
        .bind(status.as_str())
        .execute(pool)
        .await
        {
            tracing::error!(run_id = %id, error = %e, "terminal status write failed");
        }
    }
}

/// `GET /acp/runs/{run_id}/events` — replay then live events.
pub async fn run_events(
    State(s): State<Arc<AppState>>,
    Path(run_id): Path<String>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, impl IntoResponse> {
    let run = match load_run(&s.pool, &run_id).await {
        Ok(Some(r)) => r,
        Ok(None) => {
            return Err(acp_err(
                StatusCode::NOT_FOUND,
                ERR_NOT_FOUND,
                format!("run not found: {run_id}"),
            ))
        }
        Err(e) => return Err((StatusCode::INTERNAL_SERVER_ERROR, Json(e))),
    };
    let agent = agents::find_agent(&run.agent_name)
        .map(|d| d.activity_name)
        .unwrap_or_default();
    Ok(run_stream::activity_stream::<Run>(
        s.activity_tx.subscribe(),
        s.pool.clone(),
        run_id.clone(),
        a2a::task_uuid(&run_id).to_string(),
        agent.to_string(),
        run,
    ))
}

/// `GET /acp/session/{session_id}` — history = resource URLs of the
/// session's runs, newest last.
pub async fn get_session(
    State(s): State<Arc<AppState>>,
    Path(session_id): Path<String>,
) -> Result<Json<Session>, (StatusCode, Json<AcpError>)> {
    let ids = sqlx::query_scalar::<_, String>(
        "SELECT id FROM acp_runs WHERE session_id = $1 ORDER BY created_at",
    )
    .bind(&session_id)
    .fetch_all(&s.pool)
    .await
    .map_err(|e| acp_err(StatusCode::INTERNAL_SERVER_ERROR, ERR_SERVER, e.to_string()))?;
    if ids.is_empty() {
        return Err(acp_err(
            StatusCode::NOT_FOUND,
            ERR_NOT_FOUND,
            format!("session not found: {session_id}"),
        ));
    }
    Ok(Json(Session {
        id: session_id,
        history: ids.iter().map(|id| format!("/acp/runs/{id}")).collect(),
        state: None,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn input_to_text_joins_messages() {
        let input = vec![
            Message::agent_text("user", "one"),
            Message::agent_text("user", "two"),
        ];
        assert_eq!(input_to_text(&input), "one\n\ntwo");
    }

    #[test]
    fn input_to_text_falls_back_to_json() {
        let input = vec![Message::agent_text("user", "")];
        let text = input_to_text(&input);
        assert!(text.starts_with('['), "{text}");
    }
}
