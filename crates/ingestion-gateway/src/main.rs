//! Ingestion gateway for the Ares SIEM+SOAR platform.
//!
//! Exposes HTTP webhook endpoints that normalize inbound security events into
//! [`Envelope`]s and publishes them to NATS JetStream on `ares.>`.
//!
//! Endpoints:
//! - `POST /webhook/wazuh`   — Wazuh integratord JSON.
//! - `POST /webhook/generic` — arbitrary JSON event.
//! - `GET  /health`          — liveness probe.
//!
//! Optional pollers (stubbed behind env flags):
//! - `POLL_M365`  — Office 365 Unified Audit Log. Requires `M365_TENANT_ID`,
//!   `M365_CLIENT_ID`, `M365_CLIENT_SECRET`.
//! - `POLL_ENTRA` — Entra ID (Graph) sign-in logs. Requires `ENTRA_TENANT_ID`,
//!   `ENTRA_CLIENT_ID`, `ENTRA_CLIENT_SECRET`.

mod pollers;
mod ratelimit;

use std::net::SocketAddr;
use std::sync::Arc;

use agent_protocol::envelope::{Envelope, EnvelopeKind};
use agent_protocol::SUBJECT_ALERTS;
use axum::{
    extract::State,
    http::StatusCode,
    middleware,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use clap::Parser;
use serde_json::Value;
use tracing::{info, warn};

use agent_core::NatsSink;

/// Ingestion gateway service configuration.
#[derive(Debug, Parser)]
#[command(name = "ingestion-gateway", about = "Ares webhook ingestion gateway")]
struct Config {
    /// Address to bind the HTTP server to.
    #[arg(long, env = "BIND_ADDR", default_value = "0.0.0.0:8091")]
    bind_addr: String,

    /// NATS server URL.
    #[arg(long, env = "NATS_URL", default_value = "nats://localhost:4222")]
    nats_url: String,

    /// Enable the Office 365 Unified Audit Log poller.
    #[arg(long, env = "POLL_M365", default_value = "false")]
    poll_m365: bool,

    /// Enable the Entra ID sign-in log poller.
    #[arg(long, env = "POLL_ENTRA", default_value = "false")]
    poll_entra: bool,
}

/// Shared application state.
struct AppState {
    sink: NatsSink,
}

/// Liveness probe.
async fn health() -> impl IntoResponse {
    Json(serde_json::json!({"status": "ok", "service": "ingestion-gateway"}))
}

/// Accepts a Wazuh integratord alert, wraps it in an [`Envelope`], and
/// publishes it to `SUBJECT_ALERTS`.
async fn wazuh_webhook(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<Value>,
) -> impl IntoResponse {
    publish_alert(&state, "wazuh", payload).await
}

/// Accepts an arbitrary JSON event, wraps it in an [`Envelope`], and publishes
/// it to `SUBJECT_ALERTS` with a generic source tag.
async fn generic_webhook(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<Value>,
) -> impl IntoResponse {
    let source = payload
        .get("source")
        .and_then(Value::as_str)
        .unwrap_or("generic")
        .to_string();
    publish_alert(&state, &source, payload).await
}

/// Wraps `payload` in an [`Envelope`] and publishes it to `SUBJECT_ALERTS`.
async fn publish_alert(
    state: &Arc<AppState>,
    source: &str,
    payload: Value,
) -> (StatusCode, Json<Value>) {
    let envelope = Envelope::new(source, EnvelopeKind::Alert, payload);
    let id = envelope.id;
    match state.sink.publish(SUBJECT_ALERTS, &envelope).await {
        Ok(()) => {
            info!(envelope_id = %id, source, "alert ingested");
            (
                StatusCode::ACCEPTED,
                Json(serde_json::json!({"id": id, "status": "accepted"})),
            )
        }
        Err(e) => {
            warn!(error = %e, envelope_id = %id, "failed to publish alert");
            (
                StatusCode::BAD_GATEWAY,
                Json(serde_json::json!({"id": id, "status": "error", "error": e.to_string()})),
            )
        }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let config = Config::parse();
    let sink = NatsSink::connect(&config.nats_url).await?;
    let state = Arc::new(AppState { sink });

    // Optional pollers behind env flags. Both are skeletons; see pollers module.
    if config.poll_m365 {
        tokio::spawn(pollers::m365_ual_poller(state.sink.clone()));
    }
    if config.poll_entra {
        tokio::spawn(pollers::entra_signin_poller(state.sink.clone()));
    }

    // Per-key rate limits on the webhook tier (authenticated actor when keys
    // are configured, else peer IP).
    let rate_limiter = ratelimit::build();
    let app = Router::new()
        .route("/health", get(health))
        .merge(
            Router::new()
                .route("/webhook/wazuh", post(wazuh_webhook))
                .route("/webhook/generic", post(generic_webhook))
                .layer(middleware::from_fn_with_state(
                    http_limits::auth::ApiKeys::from_env(),
                    http_limits::auth::require_auth,
                )),
        )
        .layer(middleware::from_fn_with_state(
            rate_limiter,
            http_limits::middleware,
        ))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(&config.bind_addr).await?;
    info!(addr = %config.bind_addr, "ingestion-gateway listening");
    axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>()).await?;
    Ok(())
}
