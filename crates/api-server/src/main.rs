//! API server for the Ares console (port 8080).
//!
//! Read models for the operator console plus the kill-switch control:
//! - `GET  /health`
//! - `GET  /alerts?limit&status`      — OpenSearch alert search.
//! - `GET  /alerts/:id`               — single alert by `_id`.
//! - `GET  /cases`, `GET /cases/:id`  — TheHive case list/detail.
//! - `GET  /audit?limit`              — `audit_log` rows, newest first.
//! - `GET  /metrics`                  — alerts/hr, verdict mix, MTTD/MTTR.
//! - `GET  /coverage`                 — ATT&CK techniques in active content.
//! - `GET  /playbooks`, `GET /playbooks/:name` — playbook YAMLs from disk.
//! - `GET  /plans/:case_id`           — `response_actions` for a case.
//! - `GET|POST /controls/kill-switch` — read/engage/disengage the kill file.
//! - `GET  /stream/activity`          — SSE feed of NATS `ares.activity`.
//! - `GET  /vulns`, `GET /vulns/:id`, `GET /vulns/:id/history`,
//!   `GET /vulns/advisories`          — NVD vulnerability intel.
//! - `POST /cvss/score`               — local CVSS v3.1/v4.0 scoring.
//! - `GET|POST /a2a/…`                — A2A protocol: agent cards, JSON-RPC
//!   task methods, per-task SSE (see [`a2a`]).
//! - `GET|POST /acp/…`                — ACP protocol: agent manifests, run
//!   lifecycle, sessions, per-run SSE (see [`acp`]).
//!
//! NOTE: TheHive list/detail go over raw REST (`POST /api/v1/query`) because
//! `ThehiveClient` exposes only create/update methods.

mod a2a;
mod acp;
mod coverage;
mod kill;
mod playbooks;
mod ratelimit;
mod run_stream;
mod stream;
mod vulns;

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use agent_core::kill_switch_engaged;
use anyhow::Context;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    middleware,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use clap::Parser;
use connectors::{ConnectorConfig, OpensearchClient};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::PgPool;
use tower_http::cors::CorsLayer;
use tracing::info;

/// Alert index pattern searched by the console.
const ALERT_INDEX: &str = "wazuh-alerts-*";
/// Verdict index written by the triage agent.
const VERDICT_INDEX: &str = "ares-verdicts";

/// API server configuration.
#[derive(Debug, Parser)]
#[command(name = "api-server", about = "Ares console backend")]
struct Config {
    /// Address to bind the HTTP server to.
    #[arg(long, env = "BIND_ADDR", default_value = "0.0.0.0:8080")]
    bind_addr: String,

    /// NATS server URL (activity stream).
    #[arg(long, env = "NATS_URL", default_value = "nats://localhost:4222")]
    nats_url: String,

    /// Postgres connection URL.
    #[arg(long, env = "DATABASE_URL")]
    database_url: String,
}

/// Shared application state.
struct AppState {
    os: OpensearchClient,
    pool: PgPool,
    http: reqwest::Client,
    nvd: connectors::NvdClient,
    enricher: enrichment_lib::Enricher,
    activity_tx: stream::ActivityTx,
    /// Durable NATS publisher shared by the A2A and ACP endpoints.
    sink: agent_core::NatsSink,
}

/// Liveness probe.
async fn health() -> impl IntoResponse {
    Json(json!({"status": "ok", "service": "api-server"}))
}

/// Query params for `GET /alerts`.
#[derive(Debug, Deserialize)]
struct AlertsQuery {
    /// Max documents to return (default 50, cap 500).
    limit: Option<u32>,
    /// Optional verdict/status filter (`open`, `triaged`, ...).
    status: Option<String>,
}

/// `GET /alerts` — recent alerts, optionally filtered by status.
async fn list_alerts(
    State(s): State<Arc<AppState>>,
    Query(q): Query<AlertsQuery>,
) -> Result<Json<Value>, (StatusCode, String)> {
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
        .map_err(|e| (StatusCode::BAD_GATEWAY, e.to_string()))
}

/// `GET /alerts/:id` — fetch one alert document by `_id`.
async fn get_alert(
    State(s): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let body = json!({"size": 1, "query": {"ids": {"values": [id]}}});
    s.os.query(ALERT_INDEX, "", body)
        .await
        .map(Json)
        .map_err(|e| (StatusCode::BAD_GATEWAY, e.to_string()))
}

/// TheHive query API helper — `POST {THEHIVE_URL}/api/v1/query`.
async fn thehive_query(s: &AppState, query: Value) -> Result<Value, anyhow::Error> {
    let base = std::env::var("THEHIVE_URL").unwrap_or_else(|_| "http://localhost:9000".to_string());
    let key = std::env::var("THEHIVE_API_KEY").unwrap_or_default();
    let resp = s
        .http
        .post(format!("{}/api/v1/query", base.trim_end_matches('/')))
        .bearer_auth(key)
        .json(&json!({"query": [query]}))
        .send()
        .await?
        .error_for_status()?;
    Ok(resp.json().await?)
}

/// `GET /cases` — list TheHive cases, newest first.
async fn list_cases(State(s): State<Arc<AppState>>) -> Result<Json<Value>, (StatusCode, String)> {
    thehive_query(&s, json!({"_name": "listCase"}))
        .await
        .map(Json)
        .map_err(|e| (StatusCode::BAD_GATEWAY, e.to_string()))
}

/// `GET /cases/:id` — one TheHive case by id/number.
async fn get_case(
    State(s): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<Value>, (StatusCode, String)> {
    thehive_query(&s, json!({"_name": "getCase", "idOrName": id}))
        .await
        .map(Json)
        .map_err(|e| (StatusCode::BAD_GATEWAY, e.to_string()))
}

/// Query params for `GET /audit`.
#[derive(Debug, Deserialize)]
struct AuditQuery {
    /// Max rows to return (default 100, cap 1000).
    limit: Option<i64>,
}

/// `GET /audit` — newest `audit_log` rows.
async fn list_audit(
    State(s): State<Arc<AppState>>,
    Query(q): Query<AuditQuery>,
) -> Result<Json<Value>, (StatusCode, String)> {
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
    .map_err(|e| (StatusCode::BAD_GATEWAY, e.to_string()))?;
    Ok(Json(json!({"rows": rows})))
}

/// `GET /metrics` — alerts/hr, verdict mix, MTTD/MTTR from `bas_results`.
async fn metrics(State(s): State<Arc<AppState>>) -> Result<Json<Value>, (StatusCode, String)> {
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
async fn get_plan(
    State(s): State<Arc<AppState>>,
    Path(case_id): Path<String>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let mut rows: Vec<Value> = sqlx::query_scalar(
        "SELECT to_jsonb(t) FROM (
             SELECT id, case_id, tool, args, result, rollback_json, ts
             FROM response_actions WHERE case_id = $1 ORDER BY ts
         ) t",
    )
    .bind(&case_id)
    .fetch_all(&s.pool)
    .await
    .map_err(|e| (StatusCode::BAD_GATEWAY, e.to_string()))?;
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

/// The complete route table (axum 0.8 `{param}` syntax).
fn api_router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/health", get(health))
        .route("/alerts", get(list_alerts))
        .route("/alerts/{id}", get(get_alert))
        .route("/cases", get(list_cases))
        .route("/cases/{id}", get(get_case))
        .route("/audit", get(list_audit))
        .route("/metrics", get(metrics))
        .route("/coverage", get(coverage::get_coverage))
        .route("/playbooks", get(playbooks::list_playbooks))
        .route("/playbooks/{name}", get(playbooks::get_playbook))
        .route("/plans/{case_id}", get(get_plan))
        .route(
            "/controls/kill-switch",
            get(kill::get_state).post(kill::set_state),
        )
        .route("/stream/activity", get(stream::activity_sse))
        .route("/vulns", get(vulns::list_vulns))
        .route("/vulns/advisories", get(vulns::list_advisories))
        .route("/vulns/{cve_id}", get(vulns::get_vuln))
        .route("/vulns/{cve_id}/history", get(vulns::get_vuln_history))
        .route("/cvss/score", post(vulns::score_cvss))
        // A2A protocol interop.
        .route("/.well-known/agent.json", get(a2a::well_known_card))
        .route("/a2a/agents", get(a2a::list_agents))
        .route("/a2a/{agent}/.well-known/agent.json", get(a2a::agent_card))
        .route("/a2a/{agent}", post(a2a::rpc_endpoint))
        .route("/a2a/tasks/{id}/events", get(a2a::task_events))
        // ACP (Agent Communication Protocol)
        .route("/.well-known/agent.yml", get(acp::well_known_manifest))
        .route("/acp/ping", get(acp::ping))
        .route("/acp/agents", get(acp::list_agents))
        .route("/acp/agents/{name}", get(acp::get_agent))
        .route("/acp/runs", post(acp::create_run))
        .route(
            "/acp/runs/{run_id}",
            get(acp::get_run).post(acp::resume_run),
        )
        .route("/acp/runs/{run_id}/cancel", post(acp::cancel_run))
        .route("/acp/runs/{run_id}/events", get(acp::run_events))
        .route("/acp/session/{session_id}", get(acp::get_session))
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let config = Config::parse();
    let pool = PgPool::connect(&config.database_url).await?;
    a2a::ensure_schema(&pool).await?;
    acp::ensure_schema(&pool).await?;
    let conn_cfg = ConnectorConfig::from_env();
    let nats = async_nats::connect(&config.nats_url).await?;
    let sink = agent_core::NatsSink::from_client(nats.clone()).await;
    let activity_tx = stream::spawn_activity_relay(nats.clone()).await?;
    let _ = kill_switch_engaged(); // warm the check path at boot

    let state = Arc::new(AppState {
        os: conn_cfg
            .build_opensearch_client()
            .context("OPENSEARCH_URL not set")?,
        pool,
        http: reqwest::Client::builder()
            .timeout(Duration::from_secs(15))
            .build()?,
        nvd: conn_cfg.build_nvd_client(),
        enricher: enrichment_lib::Enricher::new(),
        activity_tx,
        sink,
    });

    // Per-key rate limits (peer IP until auth lands). CORS stays outermost
    // so preflights are not metered and 429s still carry CORS headers.
    let rate_limiter = ratelimit::build();
    let app = api_router()
        .layer(middleware::from_fn_with_state(
            rate_limiter,
            http_limits::middleware,
        ))
        .layer(CorsLayer::permissive())
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(&config.bind_addr).await?;
    info!(addr = %config.bind_addr, "api-server listening");
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Route registration panics on bad path syntax (e.g. axum 0.7 `:param`
    /// strings under axum 0.8); build the table to prove it is well-formed.
    #[test]
    fn api_router_registers_all_routes() {
        let _ = api_router();
    }
}
