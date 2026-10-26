//! Message envelope shared by every ARES service.

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// The category of payload an [`Envelope`] carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum EnvelopeKind {
    /// A security alert (SIEM detection, sensor event).
    Alert,
    /// An incident case opened for investigation.
    Case,
    /// Threat-intelligence material.
    Intel,
    /// A finding produced by a threat hunt.
    HuntFinding,
    /// A breach-and-attack-simulation result.
    BasResult,
    /// A command directed at an agent or service.
    Command,
}

/// The common message wrapper published on ARES NATS subjects.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Envelope {
    /// Unique identifier of this envelope.
    pub id: Uuid,
    /// Correlation identifier tying related envelopes together.
    pub correlation_id: Uuid,
    /// Name of the service or agent that produced the envelope.
    pub source: String,
    /// Category of the payload.
    pub kind: EnvelopeKind,
    /// The payload itself; shape is determined by `kind`.
    pub payload: serde_json::Value,
    /// Timestamp at which the envelope was created.
    pub ts: DateTime<Utc>,
}

impl Envelope {
    /// Create a new envelope with fresh identifiers and the current timestamp.
    ///
    /// `id` and `correlation_id` both start as newly generated UUIDs; callers
    /// correlate a chain of envelopes by overwriting `correlation_id` with the
    /// identifier of the originating message.
    pub fn new(source: impl Into<String>, kind: EnvelopeKind, payload: serde_json::Value) -> Self {
        Self {
            id: Uuid::new_v4(),
            correlation_id: Uuid::new_v4(),
            source: source.into(),
            kind,
            payload,
            ts: Utc::now(),
        }
    }
}
