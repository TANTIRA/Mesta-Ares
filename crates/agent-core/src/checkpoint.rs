//! Pluggable persistence for [`AgentState`].

use dashmap::DashMap;
use sqlx::PgPool;
use uuid::Uuid;

use crate::{error::AgentError, state::AgentState};

/// Store capable of saving and resuming [`AgentState`] by thread id.
#[async_trait::async_trait]
pub trait CheckpointStore: Send + Sync {
    /// Persist `state`, replacing any previously stored state for its thread.
    async fn save(&self, state: &AgentState) -> Result<(), AgentError>;

    /// Load the stored state for `thread_id`, or `None` if absent.
    async fn load(&self, thread_id: Uuid) -> Result<Option<AgentState>, AgentError>;
}

/// In-memory [`CheckpointStore`] backed by a [`DashMap`]; for tests and
/// ephemeral runs.
#[derive(Default)]
pub struct InMemoryCheckpointer {
    map: DashMap<Uuid, AgentState>,
}

impl InMemoryCheckpointer {
    /// Create an empty in-memory store.
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait::async_trait]
impl CheckpointStore for InMemoryCheckpointer {
    async fn save(&self, state: &AgentState) -> Result<(), AgentError> {
        self.map.insert(state.thread_id, state.clone());
        Ok(())
    }

    async fn load(&self, thread_id: Uuid) -> Result<Option<AgentState>, AgentError> {
        Ok(self.map.get(&thread_id).map(|s| s.clone()))
    }
}

/// Postgres-backed [`CheckpointStore`].
///
/// Expects the table:
/// `agent_checkpoints(thread_id uuid pk, state jsonb, updated_at timestamptz)`.
pub struct PostgresCheckpointer {
    pool: PgPool,
}

impl PostgresCheckpointer {
    /// Create a checkpointer writing to `agent_checkpoints` via `pool`.
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait::async_trait]
impl CheckpointStore for PostgresCheckpointer {
    async fn save(&self, state: &AgentState) -> Result<(), AgentError> {
        sqlx::query(
            "INSERT INTO agent_checkpoints (thread_id, state, updated_at) \
             VALUES ($1, $2, now()) \
             ON CONFLICT (thread_id) DO UPDATE \
             SET state = EXCLUDED.state, updated_at = now()",
        )
        .bind(state.thread_id)
        .bind(sqlx::types::Json(state))
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn load(&self, thread_id: Uuid) -> Result<Option<AgentState>, AgentError> {
        let row: Option<sqlx::types::Json<AgentState>> =
            sqlx::query_scalar("SELECT state FROM agent_checkpoints WHERE thread_id = $1")
                .bind(thread_id)
                .fetch_optional(&self.pool)
                .await?;
        Ok(row.map(|j| j.0))
    }
}
