//! Tamper-evident audit log for ARES.
//!
//! [`AuditWriter::append`] writes a hash-chained [`AuditEvent`] into the
//! `audit_log` Postgres table; [`AuditWriter::ship`] mirrors an event into
//! OpenSearch (`ares-audit` index) for SIEM visibility.
//!
//! Expected table:
//! `audit_log (seq bigserial pk, ts timestamptz, actor text, tool text,
//! args_sha256 text, decision text, reason text, correlation_id uuid,
//! prev_hash text, hash text)`.

#![warn(missing_docs)]

use chrono::Utc;
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use uuid::Uuid;

pub use agent_protocol::audit::AuditEvent;

/// SHA-256 hex digest of the zero hash used as `prev_hash` for the first
/// event in the chain.
const GENESIS_HASH: &str = "0000000000000000000000000000000000000000000000000000000000000000";

/// Errors the audit writer can return.
#[derive(Debug, thiserror::Error)]
pub enum AuditError {
    /// Database failure reading or writing `audit_log`.
    #[error("database error: {0}")]
    Db(#[from] sqlx::Error),

    /// HTTP failure shipping an event to OpenSearch.
    #[error("http error: {0}")]
    Http(#[from] reqwest::Error),

    /// Serialization failure canonicalizing arguments or the event.
    #[error("serialization error: {0}")]
    Json(#[from] serde_json::Error),
}

/// Appends hash-chained [`AuditEvent`]s to Postgres and ships them to
/// OpenSearch.
pub struct AuditWriter {
    pg: PgPool,
}

impl AuditWriter {
    /// Create a writer bound to `pool`.
    pub fn new(pg: PgPool) -> Self {
        Self { pg }
    }

    /// Append a hash-chained event to `audit_log`.
    ///
    /// Computes `args_sha256`, reads the previous event's `hash` (or the zero
    /// hash for the first row), chains `hash = sha256(prev_hash || canonical
    /// fields)`, inserts the row, and returns the stored event.
    #[tracing::instrument(skip(self, args), fields(actor = %actor, tool = %tool, decision = %decision))]
    pub async fn append(
        &self,
        actor: &str,
        tool: &str,
        args: &serde_json::Value,
        decision: &str,
        reason: &str,
        correlation_id: Uuid,
    ) -> Result<AuditEvent, AuditError> {
        let args_sha256 = sha256_hex(&serde_json::to_vec(args)?);

        let prev_hash: Option<String> =
            sqlx::query_scalar("SELECT hash FROM audit_log ORDER BY seq DESC LIMIT 1")
                .fetch_optional(&self.pg)
                .await?;
        let prev_hash = prev_hash.unwrap_or_else(|| GENESIS_HASH.to_string());

        let ts = Utc::now();
        let canonical = format!(
            "{prev_hash}|{actor}|{tool}|{args_sha256}|{decision}|{reason}|\
             {correlation_id}|{ts}",
            ts = ts.to_rfc3339(),
        );
        let hash = sha256_hex(canonical.as_bytes());

        let seq: i64 = sqlx::query_scalar(
            "INSERT INTO audit_log \
             (ts, actor, tool, args_sha256, decision, reason, correlation_id, \
              prev_hash, hash) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9) \
             RETURNING seq",
        )
        .bind(ts)
        .bind(actor)
        .bind(tool)
        .bind(&args_sha256)
        .bind(decision)
        .bind(reason)
        .bind(correlation_id)
        .bind(&prev_hash)
        .bind(&hash)
        .fetch_one(&self.pg)
        .await?;

        Ok(AuditEvent {
            seq,
            ts,
            actor: actor.to_string(),
            tool: tool.to_string(),
            args_sha256,
            decision: decision.to_string(),
            reason: reason.to_string(),
            correlation_id,
            prev_hash,
            hash,
        })
    }

    /// Ship `event` to OpenSearch at `{opensearch_url}/ares-audit/_doc` for
    /// SIEM visibility. Transport or non-2xx failures surface as
    /// [`AuditError::Http`]; the Postgres row is already durable.
    #[tracing::instrument(skip(self, client, event), fields(seq = event.seq))]
    pub async fn ship(
        &self,
        client: &reqwest::Client,
        opensearch_url: &str,
        event: &AuditEvent,
    ) -> Result<(), AuditError> {
        let url = format!("{}/ares-audit/_doc", opensearch_url.trim_end_matches('/'));
        client
            .post(&url)
            .json(event)
            .send()
            .await
            .map_err(AuditError::Http)?
            .error_for_status()
            .map_err(AuditError::Http)?;
        Ok(())
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_hex_is_64_lower_hex_chars() {
        let h = sha256_hex(b"ares");
        assert_eq!(h.len(), 64);
        assert!(h
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
    }

    #[test]
    fn genesis_hash_is_zero() {
        assert_eq!(GENESIS_HASH.len(), 64);
        assert!(GENESIS_HASH.chars().all(|c| c == '0'));
    }
}
