//! Detection agent for the Ares SIEM+SOAR pipeline.
//!
//! Runs the detection-as-code loop on an hourly cron **and** on demand for
//! each [`SUBJECT_DETECTION`] envelope (threat intel, BAS failures):
//!
//! gather context (recent uncovered alerts, new MISP attributes, BAS gaps)
//! → Strong LLM drafts Sigma YAML → `sigma check` (or YAML-only validation)
//! → write `detection-content/pending/{slug}.yml` → Lucene backtest over the
//! last 7 days → promote to `detection-content/active/` + git commit when
//! `fp_count <= DETECT_FP_MAX` (default 5) → deploy an OpenSearch alerting
//! monitor → audit via the `detection.sigma.deploy` tool call.
//!
//! NOTE: `MispClient::search` returns typed events; they are serialized into
//! the LLM context as JSON.

mod pipeline;

use std::sync::Arc;
use std::time::Duration;

use agent_protocol::SUBJECT_DETECTION;
use anyhow::Context;
use clap::Parser;
use connectors::{ConnectorConfig, MispClient, OpensearchClient};
use llm_client::{AnthropicProvider, LlmProvider};
use serde_json::{json, Value};
use tracing::{error, info, warn, Instrument};
use uuid::Uuid;

use agent_core::{kill_switch_engaged, publish_activity, HttpToolGateway};
use agent_protocol::activity::ActivityEvent;

/// Cron interval between autonomous detection cycles (seconds).
const CRON_INTERVAL_SECS: u64 = 3600;

/// Detection agent service configuration.
#[derive(Debug, Parser)]
#[command(name = "detection-agent", about = "Ares detection-as-code agent")]
struct Config {
    /// NATS server URL.
    #[arg(long, env = "NATS_URL", default_value = "nats://localhost:4222")]
    nats_url: String,

    /// Tool server base URL (for the `detection.sigma.deploy` audit call).
    #[arg(long, env = "TOOL_SERVER_URL", default_value = "http://localhost:8090")]
    tool_server_url: String,

    /// Maximum backtest false-positive count before a rule stays pending.
    #[arg(long, env = "DETECT_FP_MAX", default_value = "5")]
    detect_fp_max: u64,

    /// Optional Postgres URL for reading `bas_results` gaps.
    #[arg(long, env = "DATABASE_URL")]
    database_url: Option<String>,
}

/// Shared dependencies.
struct Deps {
    nats: async_nats::Client,
    provider: Arc<dyn LlmProvider>,
    os: Arc<OpensearchClient>,
    misp: Arc<MispClient>,
    gateway: Arc<HttpToolGateway>,
    pool: Option<sqlx::PgPool>,
    fp_max: u64,
}

/// Gathers detection-gap context from OpenSearch, MISP and `bas_results`.
async fn gather_context(deps: &Deps) -> Value {
    // Recent alerts with no matching coverage stand in for "closed cases
    // without coverage": the triage stream is the best signal we can query.
    let alerts = deps
        .os
        .query(
            "wazuh-alerts-*",
            "",
            json!({
                "size": 20,
                "sort": [{"@timestamp": "desc"}],
                "query": {"range": {"@timestamp": {"gte": "now-24h"}}},
                "_source": ["rule.id", "rule.description", "rule.level", "agent.name"]
            }),
        )
        .await
        .unwrap_or_else(|e| {
            warn!(error = %e, "context: alert query failed");
            Value::Null
        });

    let misp = match deps.misp.search("").await {
        Ok(events) => serde_json::to_value(events).unwrap_or(Value::Null),
        Err(e) => {
            warn!(error = %e, "context: misp search failed");
            Value::Null
        }
    };

    let bas_failures = match &deps.pool {
        Some(pool) => sqlx::query_scalar::<_, String>(
            "SELECT technique FROM bas_results WHERE detected = false
             AND ts > now() - interval '7 days'",
        )
        .fetch_all(pool)
        .await
        .unwrap_or_else(|e| {
            warn!(error = %e, "context: bas_results query failed");
            Vec::new()
        }),
        None => Vec::new(),
    };

    json!({
        "recent_alerts": alerts.pointer("/hits/hits").cloned().unwrap_or(Value::Null),
        "new_misp_attributes": misp,
        "bas_failures": bas_failures,
    })
}

/// Runs one full detection cycle and returns a summary line.
async fn run_cycle(deps: &Deps, trigger: &str, correlation_id: Uuid) -> anyhow::Result<()> {
    publish_activity(
        &deps.nats,
        &ActivityEvent::new(correlation_id, "detection", "start", trigger),
    )
    .await;

    let context = gather_context(deps).await;
    let yaml = pipeline::draft_sigma(deps.provider.as_ref(), &context).await?;
    publish_activity(
        &deps.nats,
        &ActivityEvent::new(correlation_id, "detection", "draft", "sigma drafted"),
    )
    .await;

    let doc = pipeline::validate_sigma(&yaml)?;
    let pending = pipeline::write_pending(&doc, &yaml)?;
    let title = doc
        .get("title")
        .and_then(Value::as_str)
        .unwrap_or("untitled");

    let lucene = pipeline::to_lucene(&pending, &doc)?;
    let fp_count = pipeline::backtest(&deps.os, &lucene).await?;
    publish_activity(
        &deps.nats,
        &ActivityEvent::new(
            correlation_id,
            "detection",
            "backtest",
            format!("{title}: {fp_count} hits/7d"),
        ),
    )
    .await;

    if fp_count > deps.fp_max {
        info!(
            rule = title,
            fp_count,
            max = deps.fp_max,
            "rule stays pending (noisy)"
        );
        publish_activity(
            &deps.nats,
            &ActivityEvent::new(
                correlation_id,
                "detection",
                "rejected",
                format!("{title}: {fp_count} > max"),
            ),
        )
        .await;
        return Ok(());
    }

    if kill_switch_engaged() {
        warn!(rule = title, "kill switch engaged; deployment halted");
        publish_activity(
            &deps.nats,
            &ActivityEvent::new(
                correlation_id,
                "detection",
                "kill_switch",
                format!("{title}: deployment halted"),
            ),
        )
        .await;
        return Ok(());
    }

    let active = pipeline::promote_and_commit(&pending)?;
    pipeline::deploy_monitor(&deps.os, title, &lucene).await?;
    pipeline::audit_deploy(&deps.gateway, &active, correlation_id).await;
    publish_activity(
        &deps.nats,
        &ActivityEvent::new(
            correlation_id,
            "detection",
            "deployed",
            format!("{title} active"),
        ),
    )
    .await;
    Ok(())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let config = Config::parse();
    let nats = async_nats::connect(&config.nats_url).await?;
    let conn_cfg = ConnectorConfig::from_env();
    let provider: Arc<dyn LlmProvider> = Arc::new(AnthropicProvider::from_env()?);
    let pool = match &config.database_url {
        Some(url) => Some(sqlx::PgPool::connect(url).await?),
        None => None,
    };
    let deps = Arc::new(Deps {
        nats: nats.clone(),
        provider: provider.clone(),
        os: Arc::new(
            conn_cfg
                .build_opensearch_client()
                .context("OPENSEARCH_URL not set")?,
        ),
        misp: Arc::new(conn_cfg.build_misp_client().context("MISP_URL not set")?),
        gateway: Arc::new(HttpToolGateway::new(config.tool_server_url.clone())),
        pool,
        fp_max: config.detect_fp_max,
    });

    // Hourly autonomous cycle alongside envelope-driven triggers.
    let cron_deps = deps.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(CRON_INTERVAL_SECS));
        loop {
            interval.tick().await;
            let id = Uuid::new_v4();
            let span = tracing::info_span!("detection", workflow_run_id = %id);
            if let Err(e) = run_cycle(&cron_deps, "hourly cron", id)
                .instrument(span)
                .await
            {
                error!(error = %e, "cron detection cycle failed");
            }
        }
    });

    agent_core::run_worker(&nats, SUBJECT_DETECTION, "detection", move |envelope| {
        let deps = deps.clone();
        async move { run_cycle(&deps, "envelope trigger", envelope.correlation_id).await }
    })
    .await
}
