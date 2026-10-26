//! Tamper-evident audit log for ARES.
//!
//! [`AuditWriter::append`] writes a hash-chained [`AuditEvent`] into the
//! `audit_log` Postgres table; api-server serves those rows at `GET /audit`.
//!
//! Expected table:
//! `audit_log (seq bigserial pk, ts timestamptz, actor text, tool text,
//! args_sha256 text, decision text, reason text, correlation_id uuid,
//! prev_hash text, hash text)`.

#![warn(missing_docs)]

use chrono::Utc;
use sqlx::PgPool;
use uuid::Uuid;

pub use agent_protocol::audit::AuditEvent;
use agent_protocol::tool::DecisionKind;

/// SHA-256 hex digest of the zero hash used as `prev_hash` for the first
/// event in the chain.
const GENESIS_HASH: &str = "0000000000000000000000000000000000000000000000000000000000000000";

/// Errors the audit writer can return.
#[derive(Debug, thiserror::Error)]
pub enum AuditError {
    /// Database failure reading or writing `audit_log`.
    #[error("database error: {0}")]
    Db(#[from] sqlx::Error),

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

    /// Creates `audit_log` when absent.
    pub async fn ensure_schema(pg: &PgPool) -> Result<(), AuditError> {
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS audit_log (
                seq BIGSERIAL PRIMARY KEY,
                ts TIMESTAMPTZ NOT NULL,
                actor TEXT NOT NULL,
                tool TEXT NOT NULL,
                args_sha256 TEXT NOT NULL,
                decision TEXT NOT NULL,
                reason TEXT NOT NULL,
                correlation_id UUID NOT NULL,
                prev_hash TEXT NOT NULL,
                hash TEXT NOT NULL
            )",
        )
        .execute(pg)
        .await?;
        Ok(())
    }

    /// Append a hash-chained event to `audit_log`.
    ///
    /// Computes `args_sha256`, reads the previous event's `hash` (or the zero
    /// hash for the first row), chains `hash = sha256(prev_hash || canonical
    /// fields)`, inserts the row, and returns the stored event.
    #[tracing::instrument(skip(self, args), fields(actor = %actor, tool = %tool, decision = ?decision))]
    pub async fn append(
        &self,
        actor: &str,
        tool: &str,
        args: &serde_json::Value,
        decision: DecisionKind,
        reason: &str,
        correlation_id: Uuid,
    ) -> Result<AuditEvent, AuditError> {
        let args_sha256 = sha256_hex(&serde_json::to_vec(args)?);
        let decision_label = decision.as_str();

        // Serialize chain appends: read-then-insert without a lock forks the
        // chain under concurrent /invoke traffic.
        let mut tx = self.pg.begin().await?;
        sqlx::query("SELECT pg_advisory_xact_lock(hashtext('audit_log_chain'))")
            .execute(&mut *tx)
            .await?;
        let prev_hash: Option<String> =
            sqlx::query_scalar("SELECT hash FROM audit_log ORDER BY seq DESC LIMIT 1")
                .fetch_optional(&mut *tx)
                .await?;
        let prev_hash = prev_hash.unwrap_or_else(|| GENESIS_HASH.to_string());

        // Postgres stores timestamptz at microsecond precision, so hash the
        // same precision we will read back or verify_chain sees false breaks.
        let ts = Utc::now();
        let ts = chrono::DateTime::from_timestamp_micros(ts.timestamp_micros())
            .unwrap_or(ts)
            .with_timezone(&Utc);
        let hash = sha256_hex(
            canonical_string(&HashFields {
                prev_hash: &prev_hash,
                actor,
                tool,
                args_sha256: &args_sha256,
                decision: decision_label,
                reason,
                correlation_id,
                ts: &ts,
            })
            .as_bytes(),
        );

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
        .bind(decision_label)
        .bind(reason)
        .bind(correlation_id)
        .bind(&prev_hash)
        .bind(&hash)
        .fetch_one(&mut *tx)
        .await?;
        tx.commit().await?;

        Ok(AuditEvent {
            seq,
            ts,
            actor: actor.to_string(),
            tool: tool.to_string(),
            args_sha256,
            decision,
            reason: reason.to_string(),
            correlation_id,
            prev_hash,
            hash,
        })
    }

    /// Walks the chain and reports the first break, if any.
    ///
    /// The hash covers `prev_hash|actor|tool|args_sha256|decision|reason|
    /// correlation_id|ts`, so a mutated row or a missing link is detectable.
    pub async fn verify_chain(pg: &PgPool) -> Result<ChainReport, AuditError> {
        let rows = sqlx::query_as::<_, ChainRow>(
            "SELECT seq, ts, actor, tool, args_sha256, decision, reason, \
                    correlation_id, prev_hash, hash \
             FROM audit_log ORDER BY seq",
        )
        .fetch_all(pg)
        .await?;

        let mut expected_prev = GENESIS_HASH.to_string();
        let mut verified = 0u64;
        for row in &rows {
            let canonical = canonical_string(&HashFields {
                prev_hash: &row.prev_hash,
                actor: &row.actor,
                tool: &row.tool,
                args_sha256: &row.args_sha256,
                decision: &row.decision,
                reason: &row.reason,
                correlation_id: row.correlation_id,
                ts: &row.ts,
            });
            if row.prev_hash != expected_prev {
                return Ok(ChainReport {
                    rows: rows.len() as u64,
                    verified,
                    broken_at: Some(row.seq),
                    reason: Some("prev_hash does not match the previous row".to_string()),
                });
            }
            if sha256_hex(canonical.as_bytes()) != row.hash {
                return Ok(ChainReport {
                    rows: rows.len() as u64,
                    verified,
                    broken_at: Some(row.seq),
                    reason: Some("hash does not match row contents".to_string()),
                });
            }
            expected_prev = row.hash.clone();
            verified += 1;
        }
        Ok(ChainReport {
            rows: rows.len() as u64,
            verified,
            broken_at: None,
            reason: None,
        })
    }
}

/// One `audit_log` row, as read by [`AuditWriter::verify_chain`].
#[derive(sqlx::FromRow)]
struct ChainRow {
    seq: i64,
    ts: chrono::DateTime<Utc>,
    actor: String,
    tool: String,
    args_sha256: String,
    decision: String,
    reason: String,
    correlation_id: Uuid,
    prev_hash: String,
    hash: String,
}

/// Result of a chain verification pass.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ChainReport {
    /// Rows inspected.
    pub rows: u64,
    /// Rows that verified before the first break.
    pub verified: u64,
    /// `seq` of the first broken row, if any.
    pub broken_at: Option<i64>,
    /// Why that row failed.
    pub reason: Option<String>,
}

use agent_protocol::hex::sha256_hex;

/// The fields one audit row's `hash` covers.
struct HashFields<'a> {
    prev_hash: &'a str,
    actor: &'a str,
    tool: &'a str,
    args_sha256: &'a str,
    decision: &'a str,
    reason: &'a str,
    correlation_id: Uuid,
    ts: &'a chrono::DateTime<Utc>,
}

/// The exact string a row's `hash` covers.
fn canonical_string(f: &HashFields<'_>) -> String {
    let HashFields {
        prev_hash,
        actor,
        tool,
        args_sha256,
        decision,
        reason,
        correlation_id,
        ts,
    } = *f;
    format!(
        "{prev_hash}|{actor}|{tool}|{args_sha256}|{decision}|{reason}|{correlation_id}|{ts}",
        ts = ts.to_rfc3339(),
    )
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
