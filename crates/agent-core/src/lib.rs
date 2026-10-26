//! Agent control loop for ARES.
//!
//! [`AgentLoop`] drives a checkpointed think-act-observe loop: it sends the
//! serialized [`AgentState`] to an [`llm_client::LlmProvider`], parses a JSON
//! directive (`{"action":"plan|tool|final", ...}`), invokes tools through a
//! [`ToolGateway`], and stops on a final answer, an exhausted [`Budgets`], or
//! an engaged [`kill_switch_engaged`].

#![warn(missing_docs)]

mod agent_loop;
mod budget;
mod checkpoint;
mod error;
mod gateway;
mod nats_sink;
mod state;

pub use agent_loop::{AgentLoop, LoopOutcome};
pub use budget::Budgets;
pub use checkpoint::{CheckpointStore, InMemoryCheckpointer, PostgresCheckpointer};
pub use error::AgentError;
pub use gateway::{HttpToolGateway, ToolGateway};
pub use nats_sink::{NatsSink, STREAM_NAME};
pub use agent_protocol::kill::kill_switch_engaged;
pub use state::{AgentState, Step};

/// Publishes an [`agent_protocol::activity::ActivityEvent`] to
/// `ares.activity`; failures are logged only.
pub async fn publish_activity(
    client: &async_nats::Client,
    event: &agent_protocol::activity::ActivityEvent,
) {
    match serde_json::to_vec(event) {
        Ok(bytes) => {
            if let Err(e) = client
                .publish(
                    agent_protocol::activity::SUBJECT_ACTIVITY.to_string(),
                    bytes.into(),
                )
                .await
            {
                tracing::warn!(error = %e, "activity publish failed");
            }
        }
        Err(e) => tracing::warn!(error = %e, "activity serialize failed"),
    }
}
