//! Shared per-run activity SSE machinery for A2A tasks and ACP runs.
//!
//! Both protocols store a record (`a2a_tasks` / `acp_runs`), replay it,
//! stream `ares.activity` items for its workflow run, and close with a final
//! record event once the agent reports a terminal step. This module owns that
//! state machine, the `agent_checkpoints` folding and the terminal write-back;
//! the protocol modules own only their wire shapes.

use std::convert::Infallible;
use std::time::Duration;

use agent_protocol::activity::ActivityEvent;
use axum::response::sse::{Event, KeepAlive, Sse};
use futures::Stream;
use serde_json::Value;
use sqlx::PgPool;
use tokio::sync::broadcast;

use agent_protocol::activity::{aborted_step, terminal_step};

use crate::a2a::task_uuid;

/// Progress reported by `agent_checkpoints` for a non-terminal record.
pub(crate) enum CheckpointProgress {
    /// No checkpoint row exists.
    None,
    /// The run is still working (checkpoint without a `Final` step).
    InProgress,
    /// The run finished; the text is its final answer.
    Final(String),
}

/// Folds `agent_checkpoints` progress for `id` into a coarse status.
pub(crate) async fn checkpoint_progress(
    pool: &PgPool,
    id: &str,
) -> Result<CheckpointProgress, sqlx::Error> {
    let checkpoint = sqlx::query_scalar::<_, sqlx::types::Json<agent_core::AgentState>>(
        "SELECT state FROM agent_checkpoints WHERE thread_id = $1",
    )
    .bind(task_uuid(id))
    .fetch_optional(pool)
    .await?;
    Ok(match checkpoint {
        None => CheckpointProgress::None,
        Some(cp) => match cp.0.steps.last() {
            Some(agent_core::Step::Final { text }) => CheckpointProgress::Final(text.clone()),
            Some(_) => CheckpointProgress::InProgress,
            None => CheckpointProgress::None,
        },
    })
}

/// Waits for a terminal `ares.activity` step for `corr`, or until `timeout`.
pub(crate) async fn wait_for_terminal(
    mut rx: broadcast::Receiver<String>,
    corr: &str,
    agent: &str,
    timeout: Duration,
) {
    let wait = async {
        loop {
            match rx.recv().await {
                Ok(text) => {
                    let Ok(v) = serde_json::from_str::<Value>(&text) else {
                        continue;
                    };
                    if v.get("workflow_run_id").and_then(Value::as_str) != Some(corr) {
                        continue;
                    }
                    let step = v.get("step").and_then(Value::as_str).unwrap_or_default();
                    if terminal_step(agent, step) {
                        break;
                    }
                }
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(broadcast::error::RecvError::Closed) => break,
            }
        }
    };
    let _ = tokio::time::timeout(timeout, wait).await;
}

/// A stored run/task record that can be replayed and closed over SSE.
#[async_trait::async_trait]
pub(crate) trait RunRecord: Sized + Send + 'static {
    /// Whether the record is already in a terminal state.
    fn is_terminal(&self) -> bool;
    /// SSE event replaying the record at stream start and at close.
    fn record_event(&self) -> Event;
    /// SSE event wrapping one live `ares.activity` item.
    fn activity_event(activity: Value) -> Event;
    /// Reloads the record, folding checkpoint progress.
    async fn load(pool: &PgPool, id: &str) -> Result<Option<Self>, String>;
    /// Marks the record terminal after a terminal activity step;
    /// `aborted` distinguishes a kill-switch/rejection halt from success.
    async fn mark_terminal(pool: &PgPool, id: &str, aborted: bool);
}

/// Per-stream mutable state for [`activity_stream`].
struct StreamState<R> {
    /// Live activity broadcast receiver.
    rx: broadcast::Receiver<String>,
    /// Database handle for the terminal write-back and final reload.
    pool: PgPool,
    /// Record id for the write-back.
    id: String,
    /// `workflow_run_id` the stream filters on.
    corr: String,
    /// Activity `agent` value for terminal-step matching.
    agent: String,
    /// The replayed record to emit first.
    replay: Option<R>,
    /// A terminal step was seen: emit one final record event, then close.
    closing: bool,
    /// All events emitted; end the stream.
    done: bool,
}

/// Streams one record: replay, live activity, final record event.
pub(crate) fn activity_stream<R: RunRecord>(
    rx: broadcast::Receiver<String>,
    pool: PgPool,
    id: String,
    corr: String,
    agent: String,
    replay: R,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let st = StreamState {
        rx,
        pool,
        id,
        corr,
        agent,
        replay: Some(replay),
        closing: false,
        done: false,
    };
    let stream = futures::stream::unfold(st, |mut st: StreamState<R>| async move {
        if st.done {
            return None;
        }
        if let Some(record) = st.replay.take() {
            // Already-terminal records replay their state and close.
            if record.is_terminal() {
                st.done = true;
            }
            return Some((Ok(record.record_event()), st));
        }
        if st.closing {
            st.done = true;
            if let Ok(Some(record)) = R::load(&st.pool, &st.id).await {
                return Some((Ok(record.record_event()), st));
            }
            return None;
        }
        loop {
            match st.rx.recv().await {
                Ok(text) => {
                    let event: ActivityEvent = match serde_json::from_str(&text) {
                        Ok(event) => event,
                        Err(_) => continue,
                    };
                    if event.workflow_run_id.to_string() != st.corr {
                        continue;
                    }
                    if terminal_step(&st.agent, &event.step) {
                        R::mark_terminal(&st.pool, &st.id, aborted_step(&event.step)).await;
                        st.closing = true;
                    }
                    let value = serde_json::to_value(&event).unwrap_or(Value::Null);
                    return Some((Ok(R::activity_event(value)), st));
                }
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(broadcast::error::RecvError::Closed) => return None,
            }
        }
    });
    Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("ping"),
    )
}
