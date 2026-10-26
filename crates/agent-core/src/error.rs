//! Error type shared by the agent loop, checkpointers and gateways.

/// Errors agent-core components can return.
#[derive(Debug, thiserror::Error)]
pub enum AgentError {
    /// Failure of the underlying LLM provider.
    #[error("llm error: {0}")]
    Llm(#[from] llm_client::LlmError),

    /// HTTP failure talking to the tool gateway.
    #[error("http error: {0}")]
    Http(#[from] reqwest::Error),

    /// Database failure in a checkpoint store.
    #[error("database error: {0}")]
    Db(#[from] sqlx::Error),

    /// Serialization failure (transcript, checkpoint, observation).
    #[error("serialization error: {0}")]
    Json(#[from] serde_json::Error),
}
