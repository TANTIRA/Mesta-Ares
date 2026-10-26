//! Hash-chained audit event written for every governed action.

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// An immutable, hash-chained record of a governed action.
///
/// Each event carries the SHA-256 of the previous event in `prev_hash` and
/// its own `hash`, forming a tamper-evident chain in `audit_log`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AuditEvent {
    /// Monotonic sequence number assigned by the database.
    pub seq: i64,
    /// Timestamp at which the event was recorded.
    pub ts: DateTime<Utc>,
    /// Identity of the actor that requested the action.
    pub actor: String,
    /// Name of the tool that was invoked (or attempted).
    pub tool: String,
    /// SHA-256 hex digest of the canonical serialized tool arguments.
    pub args_sha256: String,
    /// Policy decision rendered for the action (`allow`, `deny`, ...).
    pub decision: String,
    /// Reason attached to the decision.
    pub reason: String,
    /// Correlation identifier tying the event to its triggering flow.
    pub correlation_id: Uuid,
    /// `hash` of the previous event, or the zero hash for the first event.
    pub prev_hash: String,
    /// SHA-256 hex digest over `prev_hash` and the canonical event fields.
    pub hash: String,
}
