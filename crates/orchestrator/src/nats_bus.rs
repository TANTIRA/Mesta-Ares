//! Thin NATS wrapper: subscribe + envelope publish.

use agent_protocol::envelope::Envelope;
use anyhow::Context;
use async_nats::{Client, Subscriber};

/// NATS connection facade used by the orchestrator.
pub struct NatsBus {
    client: Client,
}

impl NatsBus {
    /// Connects to NATS.
    pub async fn connect(nats_url: &str) -> anyhow::Result<Self> {
        let client = async_nats::connect(nats_url)
            .await
            .with_context(|| format!("connect to NATS at {nats_url}"))?;
        Ok(Self { client })
    }

    /// Subscribes to a subject (supports wildcards like `ares.*`).
    pub async fn subscribe(&self, subject: &str) -> anyhow::Result<Subscriber> {
        Ok(self.client.subscribe(subject.to_string()).await?)
    }

    /// Publishes a JSON-serialized envelope on `subject`.
    pub async fn publish_envelope(&self, subject: &str, envelope: &Envelope) -> anyhow::Result<()> {
        let bytes = serde_json::to_vec(envelope).context("serialize envelope")?;
        self.publish_raw(subject, bytes).await
    }

    /// Publishes raw bytes on `subject` (used for DLQ passthrough).
    pub async fn publish_raw(&self, subject: &str, bytes: Vec<u8>) -> anyhow::Result<()> {
        self.client
            .publish(subject.to_string(), bytes.into())
            .await
            .context("nats publish")?;
        self.client.flush().await.context("nats flush")?;
        Ok(())
    }
}
