//! Orchestrator for the Ares SIEM+SOAR pipeline.
//!
//! Subscribes to the envelope-bearing `ares.*` subjects and routes each
//! [`Envelope`] by
//! [`EnvelopeKind`] to its downstream subject:
//! - `Alert`       → `ares.triage`
//! - `Case`        → `ares.respond`
//! - `Intel`       → `ares.detection`
//! - `HuntFinding` → `ares.hunt`
//! - `BasResult`   → `ares.detection`
//! - `Command`     → `ares.respond`
//!
//! Deduplicates alerts via Postgres (`seen_alerts`) on a SHA-256 key of
//! `source + rule.id + host + 24h bucket`. Unparseable envelopes and routing
//! failures are dead-lettered to `ares.dlq`.

mod dedupe;

use agent_protocol::envelope::{Envelope, EnvelopeKind};
use agent_protocol::{
    SUBJECT_ALERTS, SUBJECT_BAS, SUBJECT_DETECTION, SUBJECT_DLQ, SUBJECT_HUNT, SUBJECT_RESPOND,
    SUBJECT_TRIAGE,
};
use clap::Parser;
use futures::StreamExt;
use tracing::{error, info, warn, Instrument};

use crate::dedupe::DedupeStore;
use agent_core::NatsSink;

/// Orchestrator service configuration.
#[derive(Debug, Parser)]
#[command(name = "orchestrator", about = "Ares event router")]
struct Config {
    /// NATS server URL.
    #[arg(long, env = "NATS_URL", default_value = "nats://localhost:4222")]
    nats_url: String,

    /// Postgres connection URL used for the dedupe table.
    #[arg(long, env = "DATABASE_URL")]
    database_url: String,
}

/// Maps an [`EnvelopeKind`] (and payload, for BAS results) to its downstream
/// subject. `None` means the message is terminal here.
fn route_subject(envelope: &Envelope) -> Option<&'static str> {
    match envelope.kind {
        EnvelopeKind::Alert => Some(SUBJECT_TRIAGE),
        EnvelopeKind::Case => Some(SUBJECT_RESPOND),
        EnvelopeKind::Intel => Some(SUBJECT_DETECTION),
        EnvelopeKind::HuntFinding => Some(SUBJECT_HUNT),
        // Only coverage gaps need a new rule; detected runs are terminal.
        EnvelopeKind::BasResult => {
            let detected = envelope
                .payload
                .get("detected")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(true);
            (!detected).then_some(SUBJECT_DETECTION)
        }
        EnvelopeKind::Command => Some(SUBJECT_RESPOND),
    }
}

/// Handles one inbound NATS message: parse, dedupe, route.
async fn handle_message(sink: &NatsSink, dedupe: &DedupeStore, msg: async_nats::Message) {
    let envelope: Envelope = match serde_json::from_slice(&msg.payload) {
        Ok(e) => e,
        Err(e) => {
            warn!(error = %e, subject = %msg.subject, "unparseable envelope; dropping to DLQ");
            dead_letter(sink, &msg, "unparseable envelope").await;
            return;
        }
    };

    let workflow_run_id = envelope.correlation_id;
    let span = tracing::info_span!(
        "route",
        workflow_run_id = %workflow_run_id,
        envelope_id = %envelope.id,
        kind = ?envelope.kind,
    );
    async move {
        // Envelope already sits on its destination subject (a direct publish
        // — e.g. from api-server's A2A endpoint — or our own re-publish).
        // Routing it again would deliver a duplicate and loop forever.
        let Some(subject) = route_subject(&envelope) else {
            info!("envelope is terminal at this hop");
            return;
        };
        if msg.subject.as_str() == subject {
            return;
        }
        if matches!(envelope.kind, EnvelopeKind::Alert) {
            match dedupe.check_and_mark(&envelope).await {
                Ok(true) => {
                    info!("duplicate alert; skipped");
                    return;
                }
                Ok(false) => {}
                Err(e) => {
                    // Dedupe failure must not silently drop alerts; route anyway.
                    error!(error = %e, "dedupe check failed; routing anyway");
                }
            }
        }
        if let Err(e) = sink.publish(subject, &envelope).await {
            error!(error = %e, subject, "failed to route envelope; dropping to DLQ");
            dead_letter(sink, &msg, "routing failed").await;
        } else {
            info!(subject, "envelope routed");
        }
    }
    .instrument(span)
    .await
}

/// Publishes the raw payload to the DLQ, logging a failed publish.
async fn dead_letter(sink: &NatsSink, msg: &async_nats::Message, reason: &str) {
    warn!(subject = %msg.subject, reason, "dead-lettering message");
    if let Err(e) = sink.publish_bytes(SUBJECT_DLQ, msg.payload.to_vec()).await {
        error!(error = %e, "DLQ publish failed; message dropped");
    }
}

/// Subscribes to several explicit subjects and merges them into one stream.
async fn subscribe_many(
    client: &async_nats::Client,
    subjects: &[&str],
) -> anyhow::Result<impl futures::Stream<Item = async_nats::Message>> {
    let mut subs = Vec::with_capacity(subjects.len());
    for subject in subjects {
        subs.push(client.subscribe((*subject).to_string()).await?);
    }
    Ok(futures::stream::select_all(subs))
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let config = Config::parse();
    let sink = NatsSink::connect(&config.nats_url).await?;
    let dedupe = DedupeStore::connect(&config.database_url).await?;

    // Envelope-bearing subjects only: `ares.activity` (console feed),
    // `ares.vulns` (advisories) and `ares.dlq` must never be re-routed, and a
    // wildcard would also match the DLQ it publishes to.
    let subjects = [
        SUBJECT_ALERTS,
        SUBJECT_TRIAGE,
        SUBJECT_RESPOND,
        SUBJECT_DETECTION,
        SUBJECT_HUNT,
        SUBJECT_BAS,
    ];
    let mut sub = subscribe_many(sink.client(), &subjects).await?;
    info!(?subjects, "orchestrator subscribed");

    while let Some(msg) = sub.next().await {
        handle_message(&sink, &dedupe, msg).await;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn envelope(kind: EnvelopeKind, payload: serde_json::Value) -> Envelope {
        Envelope::new("test", kind, payload)
    }

    #[test]
    fn kinds_route_to_their_agents() {
        assert_eq!(
            route_subject(&envelope(EnvelopeKind::Alert, json!({}))),
            Some(SUBJECT_TRIAGE)
        );
        assert_eq!(
            route_subject(&envelope(EnvelopeKind::HuntFinding, json!({}))),
            Some(SUBJECT_HUNT)
        );
        assert_eq!(
            route_subject(&envelope(EnvelopeKind::Command, json!({}))),
            Some(SUBJECT_RESPOND)
        );
    }

    #[test]
    fn bas_results_route_only_for_coverage_gaps() {
        assert_eq!(
            route_subject(&envelope(
                EnvelopeKind::BasResult,
                json!({"detected": false})
            )),
            Some(SUBJECT_DETECTION)
        );
        assert_eq!(
            route_subject(&envelope(
                EnvelopeKind::BasResult,
                json!({"detected": true})
            )),
            None
        );
    }
}
