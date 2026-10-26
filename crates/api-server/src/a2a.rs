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
use agent_protocol::envelope::{Envelope, EnvelopeKind};
use agent_protocol::handoff::HandoffPayload;
use agent_protocol::{SUBJECT_DETECTION, SUBJECT_HUNT, SUBJECT_RESPOND, SUBJECT_TRIAGE};
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
use tracing::info;
use uuid::Uuid;

use crate::run_stream::{self, CheckpointProgress, RunRecord};
use crate::AppState;

/// Static definition of one A2A-addressable Ares agent.
pub(crate) struct AgentDef {
    /// Card name and `/a2a/{name}` path segment.
    pub(crate) name: &'static str,
    /// Card description.
    pub(crate) description: &'static str,
    /// NATS subject the agent consumes.
    pub(crate) subject: &'static str,
    /// Envelope kind that the orchestrator routes to `subject`.
    pub(crate) kind: EnvelopeKind,
    /// Value agents put in `ActivityEvent.agent` for this agent.
    pub(crate) activity_name: &'static str,
    /// `(id, name, description, tags, examples)` rows for the card's skills.
    pub(crate) skills: &'static [SkillDef],
}

/// One skill tuple in an [`AgentDef`].
type SkillDef = (
    &'static str,
    &'static str,
    &'static str,
    &'static [&'static str],
    &'static [&'static str],
);

/// The four invocable Ares agents.
pub(crate) const AGENTS: &[AgentDef] = &[
    AgentDef {
        name: "triage-agent",
        description: "Alert triage: IOC extraction, threat-intel enrichment and an LLM verdict (tp/fp/suspicious).",
        subject: SUBJECT_TRIAGE,
        kind: EnvelopeKind::Alert,
        activity_name: "triage",
        skills: &[
            ("ioc-extraction", "IOC extraction", "Extract IPs, domains, URLs, hashes and emails from alert JSON.", &["ioc", "parsing"], &["Extract indicators from this alert"]),
            ("enrichment", "IOC enrichment", "Enrich IOCs against threat-intel providers and compute a composite score.", &["enrichment", "threat-intel"], &["Score the indicators in this alert"]),
            ("verdict", "Alert verdict", "Issue a tp/fp/suspicious verdict with severity and rationale.", &["verdict", "llm"], &["Triage this Wazuh alert"]),
        ],
    },
    AgentDef {
        name: "response-agent",
        description: "Autonomous incident response: plans and executes tool-driven actions for a case.",
        subject: SUBJECT_RESPOND,
        kind: EnvelopeKind::Command,
        activity_name: "response",
        skills: &[
            ("containment", "Containment", "Plan and run containment actions (host isolation, blocklists, account disable).", &["response", "containment"], &["Contain the compromised host"]),
            ("eradication", "Eradication", "Plan and run eradication/recovery actions for an incident.", &["response", "eradication"], &["Remove the persistence mechanism"]),
            ("planning", "Response planning", "Build an ordered, policy-checked action plan for a case.", &["planning"], &["Draft a response plan for this case"]),
        ],
    },
    AgentDef {
        name: "detection-agent",
        description: "Detection-as-code: drafts, validates, backtests and deploys Sigma content.",
        subject: SUBJECT_DETECTION,
        kind: EnvelopeKind::Intel,
        activity_name: "detection",
        skills: &[
            ("sigma", "Sigma authoring", "Draft Sigma rules for uncovered threat patterns.", &["sigma", "rules"], &["Write a rule for lateral movement via SMB"]),
            ("detection-engineering", "Detection engineering", "Validate and promote detection content through the pipeline.", &["pipeline"], &["Turn this threat-intel report into a detection"]),
            ("backtesting", "Rule backtesting", "Backtest candidate rules over the last 7 days of alerts.", &["backtest", "lucene"], &["Backtest this Sigma rule"]),
        ],
    },
    AgentDef {
        name: "hunt-agent",
        description: "Threat hunting: runs ATT&CK-mapped hypotheses over alert data and opens findings.",
        subject: SUBJECT_HUNT,
        kind: EnvelopeKind::HuntFinding,
        activity_name: "hunt",
        skills: &[
            ("hypothesis", "Hunt hypotheses", "Execute the ATT&CK-mapped hypothesis set.", &["hunt", "hypothesis"], &["Hunt for persistence mechanisms"]),
            ("attack-mapping", "ATT&CK mapping", "Map hunt coverage and findings to MITRE ATT&CK techniques.", &["attack", "mitre"], &["Map recent activity to ATT&CK"]),
            ("querying", "Log querying", "Query the alert store for hunt evidence.", &["opensearch", "lucene"], &["Find beaconing in the last 24h"]),
        ],
    },
];

/// Activity steps that mean an agent's work on a run is finished, keyed by
/// `ActivityEvent.agent`. hunt-agent emits no terminal step, so its SSE
/// streams stay open until the client disconnects.
pub(crate) fn terminal_step(agent: &str, step: &str) -> bool {
    match agent {
        "triage" => matches!(step, "close" | "handoff" | "kill_switch"),
        "response" => matches!(step, "done" | "kill_switch"),
        "detection" => matches!(step, "deployed" | "rejected"),
        _ => false,
    }
}

/// Activity steps that end a run as a halt rather than a success.
pub(crate) fn aborted_step(step: &str) -> bool {
    matches!(step, "kill_switch" | "rejected")
}

/// Looks up an agent definition by its `/a2a/{name}` segment.
pub(crate) fn find_agent(name: &str) -> Option<&'static AgentDef> {
    AGENTS.iter().find(|a| a.name == name)
}

/// Builds the [`AgentCard`] for one agent.
fn card_for(def: &AgentDef) -> AgentCard {
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
        authentication: Authentication {
            schemes: vec!["bearer".to_string()],
        },
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
fn orchestrator_card() -> AgentCard {
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
        authentication: Authentication {
            schemes: vec!["bearer".to_string()],
        },
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

/// Publishes `envelope` on `subject`, JetStream-first with plain-publish
/// fallback (same policy as ingestion-gateway's `NatsSink`).
pub(crate) async fn publish(
    s: &AppState,
    subject: &str,
    envelope: &Envelope,
) -> anyhow::Result<()> {
    s.sink.publish(subject, envelope).await
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
             VALUES ($1, $2, $3, 'working', $4, now(), now())
             ON CONFLICT (id) DO NOTHING",
        )
        .bind(&task_id)
        .bind(def.name)
        .bind(&params.session_id)
        .bind(sqlx::types::Json(&params.message))
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
        if let Err(e) = publish(self.s, def.subject, &env).await {
            let _ = sqlx::query(
                "UPDATE a2a_tasks SET state = 'failed', updated_at = now() WHERE id = $1",
            )
            .bind(&task_id)
            .execute(&self.s.pool)
            .await;
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

    async fn get(&self, task_id: &str) -> Result<Option<Task>, BackendError> {
        Ok(load_task(&self.s.pool, task_id).await?.map(|(t, _)| t))
    }

    async fn cancel(&self, task_id: &str) -> Result<Option<Task>, BackendError> {
        // Only marks the row; in-flight agents do not observe the flag.
        // (known gap: agents never poll a cancellation table.)
        sqlx::query(
            "UPDATE a2a_tasks SET state = 'canceled', updated_at = now()
             WHERE id = $1 AND state NOT IN ('completed', 'failed', 'canceled')",
        )
        .bind(task_id)
        .execute(&self.s.pool)
        .await
        .map_err(|e| BackendError::Internal(e.to_string()))?;
        Ok(load_task(&self.s.pool, task_id).await?.map(|(t, _)| t))
    }
}

/// `GET /a2a/agents` — every invocable agent card.
pub async fn list_agents() -> Json<Value> {
    Json(json!(AGENTS.iter().map(card_for).collect::<Vec<_>>()))
}

/// `GET /.well-known/agent.json` — the platform-level card.
pub async fn well_known_card() -> Json<AgentCard> {
    Json(orchestrator_card())
}

/// `GET /a2a/{agent}/.well-known/agent.json` — one agent's card.
pub async fn agent_card(Path(name): Path<String>) -> Result<Json<AgentCard>, StatusCode> {
    find_agent(&name)
        .map(|def| Json(card_for(def)))
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
        Event::default().event("activity").data(activity.to_string())
    }

    async fn load(pool: &PgPool, id: &str) -> Result<Option<Self>, String> {
        load_task(pool, id)
            .await
            .map(|found| found.map(|(task, _)| task))
            .map_err(|e| e.to_string())
    }

    async fn mark_terminal(pool: &PgPool, id: &str, aborted: bool) {
        let state = if aborted { "failed" } else { "completed" };
        let _ = sqlx::query("UPDATE a2a_tasks SET state = $2, updated_at = now() WHERE id = $1")
            .bind(id)
            .bind(state)
            .execute(pool)
            .await;
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
