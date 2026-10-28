//! NATS publishing with JetStream-first semantics and plain-publish fallback.
//!
//! One implementation for every producer: the ingestion gateway, the api
//! server's A2A/ACP endpoints, and any agent that needs durable publishes.

use agent_protocol::envelope::Envelope;
use anyhow::Context;
use async_nats::{jetstream, Client};
use tracing::{info, warn};

/// Name of the JetStream stream capturing all `ares.>` subjects.
pub const STREAM_NAME: &str = "ARES";
/// Bounded stream over `ares.dlq` — dead letters never grow unbounded.
pub const DLQ_STREAM_NAME: &str = "ARES_DLQ";
/// Max retained dead letters before oldest are discarded.
pub const DLQ_MAX_MESSAGES: i64 = 100_000;

/// Publishes serialized [`Envelope`]s to NATS.
///
/// Prefers JetStream when the `ARES` stream is available; otherwise publishes
/// core NATS messages so the pipeline still functions without JetStream.
#[derive(Clone)]
pub struct NatsSink {
    client: Client,
    jetstream: Option<jetstream::Context>,
}

impl NatsSink {
    /// Connects to NATS and creates the `ARES` stream when absent.
    ///
    /// Returns a sink with no JetStream context when the server has none, in
    /// which case publishes fall back to core NATS.
    pub async fn connect(nats_url: &str) -> anyhow::Result<Self> {
        let client = async_nats::connect(nats_url)
            .await
            .with_context(|| format!("connect to NATS at {nats_url}"))?;
        let jetstream = match Self::ensure_stream(client.clone()).await {
            Ok(js) => Some(js),
            Err(e) => {
                warn!(error = %e, "JetStream unavailable; falling back to plain NATS publish");
                None
            }
        };
        Ok(Self { client, jetstream })
    }

    /// Wraps an existing client, creating the `ARES` stream when absent.
    pub async fn from_client(client: Client) -> Self {
        let jetstream = match Self::ensure_stream(client.clone()).await {
            Ok(js) => Some(js),
            Err(e) => {
                warn!(error = %e, "JetStream unavailable; falling back to plain NATS publish");
                None
            }
        };
        Self { client, jetstream }
    }

    /// The underlying client, for subscribers.
    pub fn client(&self) -> &Client {
        &self.client
    }

    /// `true` when publishes are persisted by JetStream.
    pub fn is_durable(&self) -> bool {
        self.jetstream.is_some()
    }

    /// Creates the `ARES` stream over `ares.>` when it does not already exist.
    async fn ensure_stream(client: Client) -> anyhow::Result<jetstream::Context> {
        let js = jetstream::new(client);
        match js.get_stream(STREAM_NAME).await {
            Ok(_) => info!(stream = STREAM_NAME, "JetStream stream exists"),
            Err(_) => {
                let stream = js
                    .create_stream(jetstream::stream::Config {
                        name: STREAM_NAME.to_string(),
                        subjects: vec!["ares.>".to_string()],
                        ..Default::default()
                    })
                    .await
                    .context("create ARES JetStream stream")?;
                info!(stream = %stream.cached_info().config.name, "created JetStream stream");
            }
        }
        // Dead letters are bounded: the DLQ must never grow into a storage
        // exhaustion vector. Oldest entries are discarded past the cap.
        if js.get_stream(DLQ_STREAM_NAME).await.is_err() {
            let stream = js
                .create_stream(jetstream::stream::Config {
                    name: DLQ_STREAM_NAME.to_string(),
                    subjects: vec![agent_protocol::SUBJECT_DLQ.to_string()],
                    max_messages: DLQ_MAX_MESSAGES,
                    discard: jetstream::stream::DiscardPolicy::Old,
                    ..Default::default()
                })
                .await
                .context("create ARES_DLQ JetStream stream")?;
            info!(stream = %stream.cached_info().config.name, "created bounded DLQ stream");
        }
        Ok(js)
    }

    /// Serializes `envelope` to JSON and publishes it on `subject`.
    pub async fn publish(&self, subject: &str, envelope: &Envelope) -> anyhow::Result<()> {
        let bytes = serde_json::to_vec(envelope).context("serialize envelope")?;
        self.publish_bytes(subject, bytes).await
    }

    /// Publishes raw bytes on `subject`, JetStream-first.
    pub async fn publish_bytes(&self, subject: &str, bytes: Vec<u8>) -> anyhow::Result<()> {
        match &self.jetstream {
            Some(js) => {
                js.publish(subject.to_string(), bytes.into())
                    .await
                    .context("jetstream publish")?;
            }
            None => {
                self.client
                    .publish(subject.to_string(), bytes.into())
                    .await
                    .context("nats publish")?;
            }
        }
        Ok(())
    }
}
