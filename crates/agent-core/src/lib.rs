//! Shared agent runtime for ARES.
//!
//! - [`run_worker`] owns the subscribe → parse → span → spawn loop every
//!   envelope-driven agent needs.
//! - [`HttpToolGateway`] is the policy-gated path to the tool server.
//! - [`CheckpointStore`] persists [`AgentState`] so api-server can fold run
//!   progress into A2A/ACP status.
//! - [`publish_activity`] emits `ares.activity` steps; [`kill_switch_engaged`]
//!   is the shared safety predicate.

#![warn(missing_docs)]

mod checkpoint;
mod error;
mod gateway;
mod nats_sink;
mod state;
mod worker;

pub use agent_protocol::kill::kill_switch_engaged;
pub use checkpoint::{CheckpointStore, InMemoryCheckpointer, PostgresCheckpointer};
pub use error::AgentError;
pub use gateway::{HttpToolGateway, ToolGateway};
pub use nats_sink::{NatsSink, STREAM_NAME};
pub use state::{AgentState, Step};
pub use worker::{handle_payload, run_worker};

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
