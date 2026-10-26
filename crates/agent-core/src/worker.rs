//! Shared NATS worker loop for envelope-driven agents.
//!
//! Every agent hand-rolled the same subscribe → parse → span → spawn → log
//! sequence, differing only in the subject, the span name and the handler.
//! [`run_worker`] owns that scaffolding; an agent supplies just the handler.
//!
//! Parse failures are logged per message and do not stop the loop: one bad
//! payload must not take an agent offline.

use std::future::Future;
use std::sync::Arc;

use agent_protocol::envelope::Envelope;
use anyhow::Context;
use futures::StreamExt;
use tracing::{error, info, Instrument};

/// Parses `payload`, runs `handler` inside a per-envelope span.
///
/// Split out from [`run_worker`] so the parse/span behaviour is testable
/// without a NATS connection.
pub async fn handle_payload<F, Fut>(
    payload: Vec<u8>,
    agent: &'static str,
    handler: Arc<F>,
) -> anyhow::Result<()>
where
    F: Fn(Envelope) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = anyhow::Result<()>> + Send + 'static,
{
    let envelope: Envelope = serde_json::from_slice(&payload).context("parse envelope")?;
    // Span names must be constant in `tracing`, so the agent rides as a field.
    let span = tracing::info_span!(
        "envelope",
        agent,
        workflow_run_id = %envelope.correlation_id,
        envelope_id = %envelope.id,
        kind = ?envelope.kind,
    );
    handler(envelope).instrument(span).await
}

/// Subscribes to `subject` and dispatches every envelope to `handler` in its
/// own task. Runs until the subscription closes.
///
/// `agent` is both the span name and the log field, so it should match the
/// `ActivityEvent.agent` value the handler emits (`triage`, `response`, ...).
pub async fn run_worker<F, Fut>(
    nats: &async_nats::Client,
    subject: &'static str,
    agent: &'static str,
    handler: F,
) -> anyhow::Result<()>
where
    F: Fn(Envelope) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = anyhow::Result<()>> + Send + 'static,
{
    let handler = Arc::new(handler);
    let mut sub = nats.subscribe(subject.to_string()).await?;
    info!(subject, agent, "worker subscribed");

    while let Some(msg) = sub.next().await {
        let handler = handler.clone();
        tokio::spawn(async move {
            if let Err(e) = handle_payload(msg.payload.to_vec(), agent, handler).await {
                error!(agent, error = %e, "envelope processing failed");
            }
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent_protocol::envelope::EnvelopeKind;
    use std::sync::atomic::{AtomicBool, Ordering};

    fn envelope_bytes() -> Vec<u8> {
        serde_json::to_vec(&Envelope::new(
            "test",
            EnvelopeKind::Alert,
            serde_json::json!({"rule": "x"}),
        ))
        .unwrap()
    }

    #[tokio::test]
    async fn valid_payload_runs_the_handler() {
        let ran = Arc::new(AtomicBool::new(false));
        let flag = ran.clone();
        let handler = move |envelope: Envelope| {
            let flag = flag.clone();
            async move {
                assert_eq!(envelope.payload["rule"], "x");
                flag.store(true, Ordering::SeqCst);
                Ok(())
            }
        };
        handle_payload(envelope_bytes(), "triage", Arc::new(handler))
            .await
            .unwrap();
        assert!(ran.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn malformed_payload_is_a_parse_error_and_skips_the_handler() {
        let ran = Arc::new(AtomicBool::new(false));
        let flag = ran.clone();
        let handler = move |_envelope: Envelope| {
            let flag = flag.clone();
            async move {
                flag.store(true, Ordering::SeqCst);
                Ok(())
            }
        };
        let err = handle_payload(b"not json".to_vec(), "triage", Arc::new(handler))
            .await
            .unwrap_err();
        assert!(format!("{err:#}").contains("parse envelope"), "{err:#}");
        assert!(!ran.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn handler_errors_propagate() {
        let handler = |_envelope: Envelope| async move { anyhow::bail!("boom") };
        let err = handle_payload(envelope_bytes(), "response", Arc::new(handler))
            .await
            .unwrap_err();
        assert_eq!(err.to_string(), "boom");
    }
}
