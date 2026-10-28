//! Read-model handlers for the console: alerts, cases, audit, metrics, plans.
//!
//! Split out of `main.rs` so the router and boot sequence stay readable. Every
//! handler answers failures through the shared [`crate::error::ApiError`]
//! envelope.

use std::sync::Arc;

use anyhow::Context;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::error::{api_err, ApiError};
use crate::AppState;

/// Alert index pattern searched by the console.
pub(crate) const ALERT_INDEX: &str = "wazuh-alerts-*";
/// Verdict index written by the triage agent.
pub(crate) const VERDICT_INDEX: &str = "ares-verdicts";

/// Liveness probe.
pub(crate) async fn health() -> impl IntoResponse {
    Json(json!({"status": "ok", "service": "api-server"}))
}

/// Query params for `GET /alerts`.
#[derive(Debug, Deserialize)]
pub(crate) struct AlertsQuery {
    /// Max documents to return (default 50, cap 500).
    limit: Option<u32>,
    /// Optional verdict/status filter (`open`, `triaged`, ...).
    status: Option<String>,
}

/// `GET /alerts` — recent alerts, optionally filtered by status.
pub(crate) async fn list_alerts(
    State(s): State<Arc<AppState>>,
    Query(q): Query<AlertsQuery>,
) -> Result<Json<Value>, ApiError> {
    let limit = q.limit.unwrap_or(50).min(500);
    let mut must = vec![json!({"range": {"@timestamp": {"gte": "now-24h"}}})];
    if let Some(status) = q.status.filter(|v| !v.is_empty()) {
        must.push(json!({"term": {"status.keyword": status}}));
    }
    let body = json!({
        "size": limit,
        "sort": [{"@timestamp": "desc"}],
        "query": {"bool": {"must": must}},
    });
    s.os.query(ALERT_INDEX, "", body)
        .await
        .map(Json)
        .map_err(|e| api_err(StatusCode::BAD_GATEWAY, e.to_string()))
}

/// `GET /alerts/:id` — fetch one alert document by `_id`.
pub(crate) async fn get_alert(
    State(s): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let body = json!({"size": 1, "query": {"ids": {"values": [id]}}});
    s.os.query(ALERT_INDEX, "", body)
        .await
        .map(Json)
        .map_err(|e| api_err(StatusCode::BAD_GATEWAY, e.to_string()))
}

/// TheHive query API helper — one filter through [`ThehiveClient::query`].
pub(crate) async fn thehive_query(s: &AppState, query: Value) -> Result<Value, anyhow::Error> {
    let hive = s.hive.as_ref().context("THEHIVE_URL not configured")?;
    hive.query(query).await
}

/// `GET /cases` — list TheHive cases, newest first.
pub(crate) async fn list_cases(State(s): State<Arc<AppState>>) -> Result<Json<Value>, ApiError> {
    thehive_query(&s, json!({"_name": "listCase"}))
        .await
        .map(Json)
        .map_err(|e| api_err(StatusCode::BAD_GATEWAY, e.to_string()))
}

/// `GET /cases/:id` — one TheHive case by id/number.
pub(crate) async fn get_case(
    State(s): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    thehive_query(&s, json!({"_name": "getCase", "idOrName": id}))
        .await
        .map(Json)
        .map_err(|e| api_err(StatusCode::BAD_GATEWAY, e.to_string()))
}

/// Query params for `GET /audit`.
#[derive(Debug, Deserialize)]
pub(crate) struct AuditQuery {
    /// Max rows to return (default 100, cap 1000).
    limit: Option<i64>,
}

/// `GET /audit/verify` — re-walks the hash chain, reports the first break,
/// and cross-checks the tail against the out-of-DB anchor file.
pub(crate) async fn verify_audit(
    State(s): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let report = audit::AuditWriter::verify_chain(&s.pool)
        .await
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let anchored = audit_anchor_check(&s.pool).await;
    Ok(Json(serde_json::json!({
        "rows": report.rows,
        "verified": report.verified,
        "broken_at": report.broken_at,
        "reason": report.reason,
        "anchored": anchored,
    })))
}

/// `true` when the newest anchor line matches the current chain tail.
/// `None` when no anchor file exists (feature off).
async fn audit_anchor_check(pool: &sqlx::PgPool) -> Option<bool> {
    let path =
        std::env::var("AUDIT_ANCHOR_PATH").unwrap_or_else(|_| "audit-anchor.log".into());
    let last = std::fs::read_to_string(&path)
        .ok()?
        .lines()
        .last()?
        .to_string();
    let (seq, hash) = last.split_once(',')?;
    let (seq_db, hash_db): (i64, String) =
        sqlx::query_as("SELECT seq, hash FROM audit_log ORDER BY seq DESC LIMIT 1")
            .fetch_optional(pool)
            .await
            .ok()??;
    Some(seq_db.to_string() == seq && hash_db == hash)
}

/// `GET /audit` — newest `audit_log` rows.
pub(crate) async fn list_audit(
    State(s): State<Arc<AppState>>,
    Query(q): Query<AuditQuery>,
) -> Result<Json<Value>, ApiError> {
    let limit = q.limit.unwrap_or(100).min(1000);
    let rows: Vec<Value> = sqlx::query_scalar(
        "SELECT to_jsonb(t) FROM (
             SELECT seq, ts, actor, tool, args_sha256, decision, reason,
                    correlation_id, prev_hash, hash
             FROM audit_log ORDER BY seq DESC LIMIT $1
         ) t",
    )
    .bind(limit)
    .fetch_all(&s.pool)
    .await
    .map_err(|e| api_err(StatusCode::BAD_GATEWAY, e.to_string()))?;
    Ok(Json(json!({"rows": rows})))
}

/// `GET /metrics` — alerts/hr, verdict mix, MTTD/MTTR from `bas_results`.
pub(crate) async fn metrics(State(s): State<Arc<AppState>>) -> Result<Json<Value>, ApiError> {
    let alerts_hr =
        s.os.query(
            ALERT_INDEX,
            "",
            json!({
                "size": 0,
                "track_total_hits": true,
                "query": {"range": {"@timestamp": {"gte": "now-1h"}}}
            }),
        )
        .await
        .ok()
        .and_then(|v| v.pointer("/hits/total/value").and_then(Value::as_u64))
        .unwrap_or(0);

    let verdict_mix =
        s.os.query(
            VERDICT_INDEX,
            "",
            json!({
                "size": 0,
                "aggs": {"mix": {"terms": {"field": "verdict.keyword"}}}
            }),
        )
        .await
        .unwrap_or(Value::Null);

    let bas_json: Value = sqlx::query_scalar(
        "SELECT to_jsonb(t) FROM (
             SELECT avg(mttd_secs)::float8 AS mttd_secs,
                    avg(mttr_secs)::float8 AS mttr_secs,
                    count(*) FILTER (WHERE detected) AS detected,
                    count(*) AS total
             FROM bas_results WHERE ts > now() - interval '7 days'
         ) t",
    )
    .fetch_one(&s.pool)
    .await
    .unwrap_or(Value::Null);

    Ok(Json(json!({
        "alerts_per_hour": alerts_hr,
        "verdict_mix": verdict_mix.pointer("/aggregations/mix/buckets").cloned().unwrap_or(Value::Null),
        "bas": bas_json,
    })))
}

/// `GET /plans/:case_id` — recorded response actions for a case.
///
/// Each row gets a derived `status` (`result.ok` → `success`/`failed`,
/// `queued` when no result was recorded) so plan-graph views do not have
/// to re-derive it. Additive and non-breaking.
pub(crate) async fn get_plan(
    State(s): State<Arc<AppState>>,
    Path(case_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let mut rows: Vec<Value> = sqlx::query_scalar(
        "SELECT to_jsonb(t) FROM (
             SELECT id, case_id, tool, args, result, rollback_json, ts
             FROM response_actions WHERE case_id = $1 ORDER BY ts
         ) t",
    )
    .bind(&case_id)
    .fetch_all(&s.pool)
    .await
    .map_err(|e| api_err(StatusCode::BAD_GATEWAY, e.to_string()))?;
    for row in &mut rows {
        if row.get("status").is_none() {
            let status = match row.pointer("/result/ok").and_then(Value::as_bool) {
                Some(true) => "success",
                Some(false) => "failed",
                None => "queued",
            };
            row["status"] = json!(status);
        }
    }
    Ok(Json(json!({"case_id": case_id, "actions": rows})))
}
