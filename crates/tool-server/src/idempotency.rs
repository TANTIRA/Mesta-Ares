//! Idempotency records for `POST /invoke`.
//!
//! Every allowed [`ToolCall`] carrying an `idempotency_key` passes a
//! three-state claim before the executor runs:
//!
//! | stored row         | same fingerprint            | different fingerprint |
//! |--------------------|-----------------------------|-----------------------|
//! | absent / expired   | claim row, execute          | claim row, execute    |
//! | `in_flight`        | retryable error             | mismatch error        |
//! | `complete`         | replay stored result        | mismatch error        |
//! | `failed`           | reclaim row, re-execute     | mismatch error        |
//!
//! Storage is the `tool_idempotency` Postgres table when `DATABASE_URL` is
//! set; otherwise a process-local `DashMap` with identical semantics. All
//! claims are atomic (`INSERT .. ON CONFLICT` / the `DashMap` entry API) —
//! there is no check-then-insert.

use std::time::{Duration, Instant};

use agent_protocol::tool::{ToolCall, ToolResult};
use anyhow::{Context, Result};
use dashmap::DashMap;
use serde_json::Value;
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Row};

/// Record lifetime: 72h, deliberately longer than the DLQ replay window so
/// the longest retry chain still finds the stored result instead of
/// re-executing.
pub const TTL: Duration = Duration::from_secs(72 * 3600);

/// `result.error` when a key is replayed with a different payload. This is
/// the 409-equivalent: `/invoke` still answers HTTP 200 and `decision` keeps
/// the policy verdict (the conflict is an execution-layer fact, not a policy
/// one).
pub const ERR_MISMATCH: &str = "idempotency payload mismatch";

/// `result.error` while another worker owns the key mid-execution.
pub const ERR_IN_FLIGHT: &str = "request in flight, retry later";

/// `result.error` when the backing store cannot be reached — the call fails
/// closed because executing without the record would double-run on replay.
pub const ERR_STORE: &str = "idempotency store unavailable, retry later";

/// The persisted state of one `idempotency_key`.
#[derive(Debug, Clone)]
pub enum KeyState {
    /// No live row for the key (absent or expired).
    Absent,
    /// Another caller claimed the key and has not finished.
    InFlight {
        /// Fingerprint stored when the key was claimed.
        fingerprint: String,
    },
    /// Execution finished with `ok=true`; `result` is replayed verbatim.
    Complete {
        /// Fingerprint stored when the key was claimed.
        fingerprint: String,
        /// The stored result of the first execution.
        result: ToolResult,
    },
    /// Execution finished with `ok=false`; one re-execution is allowed.
    Failed {
        /// Fingerprint stored when the key was claimed.
        fingerprint: String,
    },
}

/// What the current caller may do with the key.
#[derive(Debug)]
pub enum Claim {
    /// The caller owns the key: run the tool, then report via
    /// [`Store::finish`].
    Execute,
    /// Replay the stored [`ToolResult`] verbatim; do not re-execute.
    Replay(ToolResult),
    /// Same key, different payload: conflict.
    Mismatch,
    /// The key is mid-execution elsewhere: the caller should retry later.
    InFlight,
}

/// Pure transition for the claim path, shared by both store backends.
///
/// `Failed` with a matching fingerprint yields [`Claim::Execute`]: a
/// Stripe-style retry semantic where a transient execution error may be
/// retried once under the same key instead of being permanently poisoned.
pub fn next(state: &KeyState, fingerprint: &str) -> Claim {
    match state {
        KeyState::Absent => Claim::Execute,
        KeyState::InFlight { fingerprint: fp }
        | KeyState::Complete {
            fingerprint: fp, ..
        }
        | KeyState::Failed { fingerprint: fp }
            if fp != fingerprint =>
        {
            Claim::Mismatch
        }
        KeyState::InFlight { .. } => Claim::InFlight,
        KeyState::Complete { result, .. } => Claim::Replay(result.clone()),
        KeyState::Failed { .. } => Claim::Execute,
    }
}

/// SHA-256 over the tool name and the canonical JSON of `args`.
///
/// `serde_json::Value` maps are `BTreeMap`-backed, so serialization is
/// stable across key order. Binding `tool` keeps the idempotency scope at
/// (tool, key): a key replayed against a different tool reads as
/// [`Claim::Mismatch`] rather than cross-replaying.
pub fn fingerprint(call: &ToolCall) -> String {
    let mut hasher = Sha256::new();
    hasher.update(call.tool.as_bytes());
    hasher.update([0x1f]);
    hasher.update(serde_json::to_vec(&call.args).unwrap_or_default());
    hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// The table [`Store::Pg`] persists into; [`ensure_schema`] runs at boot.
pub const DDL: &str = "CREATE TABLE IF NOT EXISTS tool_idempotency ( \
     idempotency_key TEXT PRIMARY KEY, \
     tool            TEXT NOT NULL, \
     fingerprint     TEXT NOT NULL, \
     status          TEXT NOT NULL, \
     result          JSONB, \
     created_at      TIMESTAMPTZ NOT NULL DEFAULT now(), \
     expires_at      TIMESTAMPTZ NOT NULL \
 )";

/// Create the `tool_idempotency` table when it does not exist.
pub async fn ensure_schema(pool: &PgPool) -> Result<()> {
    sqlx::query(DDL)
        .execute(pool)
        .await
        .context("ensure tool_idempotency table")?;
    Ok(())
}

/// Storage backend for idempotency records.
pub enum Store {
    /// Durable, shared across replicas: the `tool_idempotency` table.
    Pg(PgPool),
    /// Process-local fallback for `DATABASE_URL`-less runs. Same semantics,
    /// but records do not survive restarts or span replicas.
    Mem(DashMap<String, Record>),
}

/// One in-memory record, mirroring a `tool_idempotency` row.
#[derive(Debug)]
struct Record {
    fingerprint: String,
    status: Status,
    result: Option<ToolResult>,
    expires_at: Instant,
}

/// The three record states; `result` distinguishes `Complete` from `Failed`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Status {
    InFlight,
    Complete,
    Failed,
}

impl Record {
    fn in_flight(fingerprint: &str) -> Self {
        Self {
            fingerprint: fingerprint.to_string(),
            status: Status::InFlight,
            result: None,
            expires_at: Instant::now() + TTL,
        }
    }
}

impl Store {
    /// Postgres-backed store.
    pub fn pg(pool: PgPool) -> Self {
        Self::Pg(pool)
    }

    /// In-memory store with identical semantics.
    pub fn mem() -> Self {
        Self::Mem(DashMap::new())
    }

    /// Claim `key` for `tool`/`fingerprint`. On [`Claim::Execute`] the
    /// caller owns the record and must call [`Store::finish`].
    pub async fn claim(&self, key: &str, tool: &str, fingerprint: &str) -> Result<Claim> {
        match self {
            Store::Pg(pool) => claim_pg(pool, key, tool, fingerprint).await,
            Store::Mem(map) => Ok(claim_mem(map, key, fingerprint)),
        }
    }

    /// Persist the outcome of an owned claim: `ok=true` stores
    /// `status=complete`, `ok=false` stores `status=failed`.
    pub async fn finish(&self, key: &str, result: &ToolResult) -> Result<()> {
        match self {
            Store::Pg(pool) => finish_pg(pool, key, result).await,
            Store::Mem(map) => {
                finish_mem(map, key, result);
                Ok(())
            }
        }
    }
}

/// Postgres claim: a single `INSERT .. ON CONFLICT` wins fresh keys and
/// atomically reclaims expired rows and same-fingerprint `failed` rows.
/// Anything else resolves through [`next`].
async fn claim_pg(pool: &PgPool, key: &str, tool: &str, fingerprint: &str) -> Result<Claim> {
    let expires_at = chrono::Utc::now() + chrono::Duration::seconds(TTL.as_secs() as i64);

    let claimed = sqlx::query(
        "INSERT INTO tool_idempotency \
             (idempotency_key, tool, fingerprint, status, created_at, expires_at) \
         VALUES ($1, $2, $3, 'in_flight', now(), $4) \
         ON CONFLICT (idempotency_key) DO UPDATE SET \
             tool        = EXCLUDED.tool, \
             fingerprint = EXCLUDED.fingerprint, \
             status      = 'in_flight', \
             result      = NULL, \
             created_at  = now(), \
             expires_at  = EXCLUDED.expires_at \
         WHERE tool_idempotency.expires_at < now() \
            OR (tool_idempotency.status = 'failed' \
                AND tool_idempotency.fingerprint = EXCLUDED.fingerprint) \
         RETURNING idempotency_key",
    )
    .bind(key)
    .bind(tool)
    .bind(fingerprint)
    .bind(expires_at)
    .fetch_optional(pool)
    .await
    .context("claim idempotency key")?;

    if claimed.is_some() {
        return Ok(Claim::Execute);
    }

    // A live row holds the key; inspect it and let the state machine decide.
    let row = sqlx::query(
        "SELECT status, fingerprint, result FROM tool_idempotency \
         WHERE idempotency_key = $1",
    )
    .bind(key)
    .fetch_optional(pool)
    .await
    .context("read idempotency row")?;

    let Some(row) = row else {
        // The row vanished between the conflict and the read. Rows are
        // reclaimed, never deleted, so this should not happen; the safe
        // answer is retryable rather than a blind re-execution.
        return Ok(Claim::InFlight);
    };

    let status: String = row.get("status");
    let stored_fp: String = row.get("fingerprint");
    let state = match status.as_str() {
        "in_flight" => KeyState::InFlight {
            fingerprint: stored_fp,
        },
        "complete" => match row
            .get::<Option<Value>, _>("result")
            .and_then(|v| serde_json::from_value::<ToolResult>(v).ok())
        {
            Some(result) => KeyState::Complete {
                fingerprint: stored_fp,
                result,
            },
            // Stored result is unreadable: heal the key by re-executing.
            None => KeyState::Failed {
                fingerprint: stored_fp,
            },
        },
        "failed" => KeyState::Failed {
            fingerprint: stored_fp,
        },
        // Unknown status: do not re-execute, tell the caller to retry.
        _ => KeyState::InFlight {
            fingerprint: stored_fp,
        },
    };

    match next(&state, fingerprint) {
        Claim::Execute => {
            // The row flipped to `failed` after our INSERT lost the race.
            // Reclaim it with a guarded UPDATE so only one retry wins.
            let won = sqlx::query(
                "UPDATE tool_idempotency SET \
                     status = 'in_flight', result = NULL, \
                     created_at = now(), expires_at = $2 \
                 WHERE idempotency_key = $1 \
                   AND status = 'failed' AND fingerprint = $3",
            )
            .bind(key)
            .bind(expires_at)
            .bind(fingerprint)
            .execute(pool)
            .await
            .context("reclaim failed idempotency row")?;
            Ok(if won.rows_affected() == 1 {
                Claim::Execute
            } else {
                Claim::InFlight
            })
        }
        claim => Ok(claim),
    }
}

async fn finish_pg(pool: &PgPool, key: &str, result: &ToolResult) -> Result<()> {
    let status = if result.ok { "complete" } else { "failed" };
    let body = serde_json::to_value(result).context("serialize tool result")?;
    let updated = sqlx::query(
        "UPDATE tool_idempotency SET status = $2, result = $3 \
         WHERE idempotency_key = $1 AND status = 'in_flight'",
    )
    .bind(key)
    .bind(status)
    .bind(body)
    .execute(pool)
    .await
    .context("finish idempotency record")?;
    if updated.rows_affected() == 0 {
        anyhow::bail!("idempotency row `{key}` no longer in_flight");
    }
    Ok(())
}

/// In-memory claim: the `DashMap` entry API holds the shard write lock, so
/// claim-then-decide is atomic like the Postgres `ON CONFLICT` path.
fn claim_mem(map: &DashMap<String, Record>, key: &str, fingerprint: &str) -> Claim {
    match map.entry(key.to_string()) {
        dashmap::Entry::Vacant(slot) => {
            slot.insert(Record::in_flight(fingerprint));
            Claim::Execute
        }
        dashmap::Entry::Occupied(mut slot) => {
            let rec = slot.get();
            if rec.expires_at <= Instant::now() {
                slot.insert(Record::in_flight(fingerprint));
                return Claim::Execute;
            }
            let state = match rec.status {
                Status::InFlight => KeyState::InFlight {
                    fingerprint: rec.fingerprint.clone(),
                },
                Status::Complete => match &rec.result {
                    Some(result) => KeyState::Complete {
                        fingerprint: rec.fingerprint.clone(),
                        result: result.clone(),
                    },
                    None => KeyState::Failed {
                        fingerprint: rec.fingerprint.clone(),
                    },
                },
                Status::Failed => KeyState::Failed {
                    fingerprint: rec.fingerprint.clone(),
                },
            };
            match next(&state, fingerprint) {
                Claim::Execute => {
                    let rec = slot.get_mut();
                    rec.status = Status::InFlight;
                    rec.result = None;
                    rec.expires_at = Instant::now() + TTL;
                    Claim::Execute
                }
                claim => claim,
            }
        }
    }
}

fn finish_mem(map: &DashMap<String, Record>, key: &str, result: &ToolResult) {
    if let Some(mut rec) = map.get_mut(key) {
        if rec.status == Status::InFlight {
            rec.status = if result.ok {
                Status::Complete
            } else {
                Status::Failed
            };
            rec.result = Some(result.clone());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn tool_result(ok: bool) -> ToolResult {
        ToolResult {
            call_id: Uuid::nil(),
            ok,
            output: serde_json::json!({"echo": 1}),
            error: (!ok).then(|| "boom".to_string()),
        }
    }

    fn call(tool: &str, args: Value) -> ToolCall {
        ToolCall {
            id: Uuid::nil(),
            tool: tool.to_string(),
            args,
            actor: "test".to_string(),
            idempotency_key: "key".to_string(),
            correlation_id: Uuid::nil(),
        }
    }

    #[test]
    fn absent_key_executes() {
        assert!(matches!(next(&KeyState::Absent, "fp"), Claim::Execute));
    }

    #[test]
    fn in_flight_same_fingerprint_is_retryable() {
        let state = KeyState::InFlight {
            fingerprint: "fp".to_string(),
        };
        assert!(matches!(next(&state, "fp"), Claim::InFlight));
    }

    #[test]
    fn different_fingerprint_mismatches_in_any_state() {
        for state in [
            KeyState::InFlight {
                fingerprint: "a".to_string(),
            },
            KeyState::Complete {
                fingerprint: "a".to_string(),
                result: tool_result(true),
            },
            KeyState::Failed {
                fingerprint: "a".to_string(),
            },
        ] {
            assert!(matches!(next(&state, "b"), Claim::Mismatch), "{state:?}");
        }
    }

    #[test]
    fn complete_same_fingerprint_replays_stored_result() {
        let stored = tool_result(true);
        let state = KeyState::Complete {
            fingerprint: "fp".to_string(),
            result: stored.clone(),
        };
        match next(&state, "fp") {
            Claim::Replay(replayed) => {
                assert!(replayed.ok);
                assert_eq!(replayed.output, stored.output);
            }
            other => panic!("expected replay, got {other:?}"),
        }
    }

    #[test]
    fn failed_same_fingerprint_reexecutes() {
        let state = KeyState::Failed {
            fingerprint: "fp".to_string(),
        };
        assert!(matches!(next(&state, "fp"), Claim::Execute));
    }

    #[test]
    fn fingerprint_binds_tool_and_args() {
        let a = fingerprint(&call(
            "thehive.create_case",
            serde_json::json!({"x": 1, "y": [2]}),
        ));
        // Object key order is canonicalized away.
        assert_eq!(
            a,
            fingerprint(&call(
                "thehive.create_case",
                serde_json::json!({"y": [2], "x": 1})
            ))
        );
        assert_ne!(
            a,
            fingerprint(&call(
                "thehive.create_case",
                serde_json::json!({"x": 2, "y": [2]})
            ))
        );
        // Same args under a different tool is a different scope.
        assert_ne!(
            a,
            fingerprint(&call(
                "wazuh.active_response",
                serde_json::json!({"x": 1, "y": [2]})
            ))
        );
    }

    #[tokio::test]
    async fn mem_store_replays_finished_result() {
        let store = Store::mem();
        assert!(matches!(
            store.claim("k", "t", "fp").await.unwrap(),
            Claim::Execute
        ));
        assert!(matches!(
            store.claim("k", "t", "fp").await.unwrap(),
            Claim::InFlight
        ));
        store.finish("k", &tool_result(true)).await.unwrap();
        match store.claim("k", "t", "fp").await.unwrap() {
            Claim::Replay(r) => assert!(r.ok),
            other => panic!("expected replay, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn mem_store_reexecutes_after_failure() {
        let store = Store::mem();
        assert!(matches!(
            store.claim("k", "t", "fp").await.unwrap(),
            Claim::Execute
        ));
        store.finish("k", &tool_result(false)).await.unwrap();
        assert!(matches!(
            store.claim("k", "t", "fp").await.unwrap(),
            Claim::Execute
        ));
    }

    #[tokio::test]
    async fn mem_store_detects_mismatch() {
        let store = Store::mem();
        assert!(matches!(
            store.claim("k", "t", "a").await.unwrap(),
            Claim::Execute
        ));
        assert!(matches!(
            store.claim("k", "t", "b").await.unwrap(),
            Claim::Mismatch
        ));
    }
}
