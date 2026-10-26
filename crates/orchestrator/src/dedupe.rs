//! Postgres-backed alert deduplication.
//!
//! Table: `seen_alerts(dedupe_key text pk, first_seen timestamptz)`.
//! Key = sha256(source + rule.id + host + 24h_bucket).

use agent_protocol::envelope::Envelope;
use chrono::{DateTime, Datelike, Utc};
use sha2::{Digest, Sha256};
use sqlx::PgPool;

/// Stores and queries dedupe keys.
pub struct DedupeStore {
    pool: PgPool,
}

impl DedupeStore {
    /// Connects to Postgres and ensures the `seen_alerts` table exists.
    pub async fn connect(database_url: &str) -> anyhow::Result<Self> {
        let pool = PgPool::connect(database_url).await?;
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS seen_alerts (
                dedupe_key TEXT PRIMARY KEY,
                first_seen TIMESTAMPTZ NOT NULL DEFAULT now()
            )",
        )
        .execute(&pool)
        .await?;
        Ok(Self { pool })
    }

    /// Returns `true` when the alert was already seen (duplicate), `false`
    /// when this is the first occurrence (the key is recorded).
    pub async fn check_and_mark(&self, envelope: &Envelope) -> anyhow::Result<bool> {
        let key = dedupe_key(envelope);
        let res =
            sqlx::query("INSERT INTO seen_alerts (dedupe_key) VALUES ($1) ON CONFLICT DO NOTHING")
                .bind(&key)
                .execute(&self.pool)
                .await?;
        Ok(res.rows_affected() == 0)
    }
}

/// Computes the dedupe key for an alert envelope.
///
/// Format: sha256(source + rule.id + host + 24h_bucket). The 24h bucket is
/// `YYYY-MM-DD` of the envelope timestamp, so the same alert re-firing on a
/// later day is not suppressed.
fn dedupe_key(envelope: &Envelope) -> String {
    let rule_id = envelope
        .payload
        .get("rule")
        .and_then(|r| r.get("id"))
        .and_then(|v| {
            v.as_str()
                .map(str::to_string)
                .or_else(|| Some(v.to_string()))
        })
        .unwrap_or_default();
    let host = envelope
        .payload
        .get("agent")
        .and_then(|a| a.get("name"))
        .and_then(|v| v.as_str())
        .or_else(|| envelope.payload.get("host").and_then(|v| v.as_str()))
        .unwrap_or_default();
    let bucket = day_bucket(envelope.ts);
    let mut hasher = Sha256::new();
    hasher.update(envelope.source.as_bytes());
    hasher.update(rule_id.as_bytes());
    hasher.update(host.as_bytes());
    hasher.update(bucket.as_bytes());
    hex::encode(hasher.finalize())
}

/// Renders the 24h dedupe bucket (`YYYY-MM-DD`) for a timestamp.
fn day_bucket(ts: DateTime<Utc>) -> String {
    format!("{:04}-{:02}-{:02}", ts.year(), ts.month(), ts.day())
}

/// Minimal hex encoder to avoid an extra dependency.
mod hex {
    /// Encodes bytes as lowercase hex.
    pub fn encode(bytes: impl AsRef<[u8]>) -> String {
        agent_protocol::hex::hex_lower(bytes.as_ref())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent_protocol::envelope::EnvelopeKind;
    use serde_json::json;

    fn alert(rule: &str, host: &str, ts: DateTime<Utc>) -> Envelope {
        let mut env = Envelope::new(
            "test",
            EnvelopeKind::Alert,
            json!({"rule": {"id": rule}, "agent": {"name": host}}),
        );
        env.ts = ts;
        env
    }

    #[test]
    fn same_alert_same_day_dedupes() {
        let day = Utc::now();
        let a = alert("5710", "web-01", day);
        let b = alert("5710", "web-01", day);
        assert_eq!(dedupe_key(&a), dedupe_key(&b));
    }

    #[test]
    fn different_host_or_rule_does_not_dedupe() {
        let day = Utc::now();
        let base = dedupe_key(&alert("5710", "web-01", day));
        assert_ne!(base, dedupe_key(&alert("5710", "web-02", day)));
        assert_ne!(base, dedupe_key(&alert("5711", "web-01", day)));
    }

    #[test]
    fn next_day_is_a_new_key() {
        let today = Utc::now();
        let tomorrow = today + chrono::Duration::days(1);
        assert_ne!(
            dedupe_key(&alert("5710", "web-01", today)),
            dedupe_key(&alert("5710", "web-01", tomorrow))
        );
    }
}
