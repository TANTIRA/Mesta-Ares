//! NATS publishing sink with JetStream-first and plain-publish fallback.

use agent_protocol::envelope::Envelope;
use anyhow::Context;
use async_nats::{jetstream, Client};
use tracing::{info, warn};

/// Name of the JetStream stream capturing all `ares.>` subjects.
const STREAM_NAME: &str = "ARES";

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
    /// Returns a sink with `jetstream` set to `None` when JetStream is
    /// unavailable, in which case [`NatsSink::publish`] falls back to plain
    /// NATS publishes.
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
        Ok(js)
    }

    /// Serializes `envelope` to JSON and publishes it on `subject`.
    ///
    /// Uses JetStream publish when available so messages are persisted;
    /// otherwise publishes a core NATS message.
    pub async fn publish(&self, subject: &str, envelope: &Envelope) -> anyhow::Result<()> {
        let bytes = serde_json::to_vec(envelope).context("serialize envelope")?;
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
