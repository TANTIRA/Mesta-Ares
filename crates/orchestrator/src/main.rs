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
//! `source + rule.id + host + 24h bucket`. Messages redelivered more than
//! `MAX_DELIVERIES` times are dropped to `ares.dlq`.

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

/// Maximum number of delivery attempts before a message is sent to the DLQ.
const MAX_DELIVERIES: u64 = 3;

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

/// Maps an [`EnvelopeKind`] to its downstream subject.
fn route_subject(kind: &EnvelopeKind) -> &'static str {
    match kind {
        EnvelopeKind::Alert => SUBJECT_TRIAGE,
        EnvelopeKind::Case => SUBJECT_RESPOND,
        EnvelopeKind::Intel => SUBJECT_DETECTION,
        EnvelopeKind::HuntFinding => SUBJECT_HUNT,
        EnvelopeKind::BasResult => SUBJECT_DETECTION,
        EnvelopeKind::Command => SUBJECT_RESPOND,
    }
}

/// Handles one inbound NATS message: parse, dedupe, route.
async fn handle_message(sink: &NatsSink, dedupe: &DedupeStore, msg: async_nats::Message) {
    let deliveries = delivery_count(&msg);
    if deliveries > MAX_DELIVERIES {
        warn!(deliveries, "max redeliveries reached; dropping to DLQ");
        dead_letter(sink, &msg, "max redeliveries").await;
        return;
    }

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
        let subject = route_subject(&envelope.kind);
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

/// Extracts a delivery-count hint from NATS message headers, if present.
///
/// JetStream populates `Nats-Msg-Id`/consumer metadata via the ack context;
/// for core NATS the header is absent and this returns 1.
fn delivery_count(msg: &async_nats::Message) -> u64 {
    msg.headers
        .as_ref()
        .and_then(|h| h.get("Nats-Delivery-Count"))
        .map(|v| v.to_string())
        .and_then(|v| v.parse().ok())
        .unwrap_or(1)
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
