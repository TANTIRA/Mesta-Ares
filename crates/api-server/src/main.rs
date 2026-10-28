//! API server for the Ares console (port 8080).
//!
//! Read models for the operator console plus the kill-switch control:
//! - `GET  /health`
//! - `GET  /alerts?limit&status`      — OpenSearch alert search.
//! - `GET  /alerts/:id`               — single alert by `_id`.
//! - `GET  /cases`, `GET /cases/:id`  — TheHive case list/detail.
//! - `GET  /audit?limit`              — `audit_log` rows, newest first.
//! - `GET  /audit/verify`             — re-walk the audit hash chain.
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
mod agents;
mod coverage;
mod error;
mod handlers;
mod kill;
mod playbooks;
mod ratelimit;
mod run_stream;
mod stream;
mod vulns;
mod yaml_dir;

use std::net::SocketAddr;
use std::sync::Arc;

use agent_core::kill_switch_engaged;
use anyhow::Context;
use axum::{
    middleware,
    routing::{get, post},
    Router,
};
use clap::Parser;
use connectors::{ConnectorConfig, OpensearchClient};
use sqlx::PgPool;
use tower_http::cors::CorsLayer;
use tracing::info;

/// API server configuration.
#[derive(Debug, Parser)]
#[command(name = "api-server", about = "Ares console backend")]
struct Config {
    /// Address to bind the HTTP server to.
    #[arg(long, env = "BIND_ADDR", default_value = "0.0.0.0:8080")]
    pub(crate) bind_addr: String,

    /// NATS server URL (activity stream).
    #[arg(long, env = "NATS_URL", default_value = "nats://localhost:4222")]
    pub(crate) nats_url: String,

    /// Postgres connection URL.
    #[arg(long, env = "DATABASE_URL")]
    pub(crate) database_url: String,
}

/// Shared application state.
pub(crate) struct AppState {
    pub(crate) os: OpensearchClient,
    pub(crate) pool: PgPool,
    pub(crate) nvd: connectors::NvdClient,
    pub(crate) enricher: enrichment_lib::Enricher,
    pub(crate) activity_tx: stream::ActivityTx,
    /// Durable NATS publisher shared by the A2A and ACP endpoints.
    pub(crate) sink: agent_core::NatsSink,
    /// Whether inbound authentication is configured (advertised on cards).
    pub(crate) auth_enabled: bool,
    /// Audit writer for control-plane changes; `None` when the DB is down.
    pub(crate) audit: Option<Arc<audit::AuditWriter>>,
    /// TheHive client for the case read model; `None` without credentials.
    pub(crate) hive: Option<connectors::ThehiveClient>,
}

/// Restrictive-by-default CORS: `CORS_ALLOW_ORIGIN` (exact origin) opts a
/// browser origin back in; absent, no cross-origin reads are allowed.
fn cors_layer() -> CorsLayer {
    match std::env::var("CORS_ALLOW_ORIGIN") {
        Ok(origin) => CorsLayer::new()
            .allow_origin(
                origin
                    .parse::<axum::http::HeaderValue>()
                    .expect("CORS_ALLOW_ORIGIN must be a valid origin, e.g. http://localhost:3000"),
            )
            .allow_methods(tower_http::cors::Any)
            .allow_headers(tower_http::cors::Any),
        Err(_) => CorsLayer::new(),
    }
}

/// The complete route table (axum 0.8 `{param}` syntax).
fn api_router() -> Router<Arc<AppState>> {
    // `/health` stays open so probes work without credentials; every other
    // route sits behind `http_limits::auth`.
    Router::new()
        .route("/health", get(handlers::health))
        .merge(protected_router().layer(middleware::from_fn_with_state(
            http_limits::auth::ApiKeys::from_env().enforced(),
            http_limits::auth::require_auth,
        )))
}

/// Every route except `/health`.
fn protected_router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/alerts", get(handlers::list_alerts))
        .route("/alerts/{id}", get(handlers::get_alert))
        .route("/cases", get(handlers::list_cases))
        .route("/cases/{id}", get(handlers::get_case))
        .route("/audit", get(handlers::list_audit))
        .route("/audit/verify", get(handlers::verify_audit))
        .route("/metrics", get(handlers::metrics))
        .route("/coverage", get(coverage::get_coverage))
        .route("/playbooks", get(playbooks::list_playbooks))
        .route("/playbooks/{name}", get(playbooks::get_playbook))
        .route("/plans/{case_id}", get(handlers::get_plan))
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
    let audit = Some(Arc::new(audit::AuditWriter::new(pool.clone())));

    let state = Arc::new(AppState {
        os: conn_cfg
            .build_opensearch_client()
            .context("OPENSEARCH_URL not set")?,
        pool,
        nvd: conn_cfg.build_nvd_client(),
        enricher: enrichment_lib::Enricher::new(),
        activity_tx,
        sink,
        auth_enabled: http_limits::auth::ApiKeys::from_env()
            .enforced()
            .is_enabled(),
        audit,
        hive: conn_cfg.build_thehive_client(),
    });

    // Per-key rate limits (peer IP until auth lands). CORS stays outermost
    // so preflights are not metered and 429s still carry CORS headers.
    let rate_limiter = ratelimit::build();
    let app = api_router()
        .layer(middleware::from_fn_with_state(
            rate_limiter,
            http_limits::middleware,
        ))
        // The console reaches us same-origin through its BFF proxy, so CORS
        // is locked down by default; CORS_ALLOW_ORIGIN re-enables one origin
        // for tooling that calls the API straight from a browser.
        .layer(cors_layer())
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
