//! Error type shared by the checkpointers and gateways.

/// Errors agent-core components can return.
#[derive(Debug, thiserror::Error)]
pub enum AgentError {
    /// HTTP failure talking to the tool gateway.
    #[error("http error: {0}")]
    Http(#[from] reqwest::Error),

    /// Database failure in a checkpoint store.
    #[error("database error: {0}")]
    Db(#[from] sqlx::Error),

    /// Serialization failure (transcript, checkpoint, observation).
    #[error("serialization error: {0}")]
    Json(#[from] serde_json::Error),

    /// The tool server answered with a non-2xx status (outage, not a verdict).
    #[error("tool gateway returned {status}: {body}")]
    Gateway {
        /// HTTP status code.
        status: u16,
        /// Response body, truncated by the server.
        body: String,
    },

    /// The policy gate denied the call.
    #[error("tool call denied: {0}")]
    Denied(String),

    /// The tool ran but reported a failure.
    #[error("tool failed: {0}")]
    Tool(String),
}
