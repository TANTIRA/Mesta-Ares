//! Hunt agent for the Ares SIEM+SOAR pipeline.
//!
//! Runs ATT&CK-mapped hunt hypotheses (see [`hypotheses`]) on a cron
//! (`HUNT_INTERVAL_SECS`, default 6h) and on [`SUBJECT_HUNT`] triggers.
//! Each hypothesis renders to an OpenSearch query; hits are scored and, when
//! the score clears a bar, a TheHive findings case tagged `hunt` is opened.
//! Progress goes to `ares.activity`.

mod hypotheses;

use std::sync::Arc;
use std::time::Duration;

use agent_protocol::envelope::Envelope;
use agent_protocol::SUBJECT_HUNT;
use anyhow::Context;
use clap::Parser;
use connectors::{ConnectorConfig, NewCase, OpensearchClient};
use futures::StreamExt;
use serde_json::Value;
use tracing::{error, info, warn, Instrument};
use uuid::Uuid;

use agent_core::{kill_switch_engaged, publish_activity, HttpToolGateway};
use agent_protocol::activity::ActivityEvent;
use agent_protocol::tool::ToolCall;

/// Default hunt interval: 6 hours.
const DEFAULT_HUNT_INTERVAL_SECS: u64 = 6 * 3600;
/// Index pattern the hunts search.
const HUNT_INDEX: &str = "wazuh-alerts-*";
/// Minimum finding score to open a case.
const CASE_THRESHOLD: u64 = 5;

/// Hunt agent service configuration.
#[derive(Debug, Parser)]
#[command(name = "hunt-agent", about = "Ares threat-hunt agent")]
struct Config {
    /// NATS server URL.
    #[arg(long, env = "NATS_URL", default_value = "nats://localhost:4222")]
    nats_url: String,

    /// Tool-server base URL (case creation goes through its policy gate).
    #[arg(long, env = "TOOL_SERVER_URL", default_value = "http://localhost:8090")]
    tool_server_url: String,

    /// Seconds between autonomous hunt sweeps.
    #[arg(long, env = "HUNT_INTERVAL_SECS", default_value_t = DEFAULT_HUNT_INTERVAL_SECS)]
    hunt_interval_secs: u64,
}

/// Shared dependencies.
struct Deps {
    nats: async_nats::Client,
    os: Arc<OpensearchClient>,
    gateway: Arc<HttpToolGateway>,
}


/// Scores a hunt result set: distinct-host hits weigh more than repeat noise.
///
/// SIMPLIFICATION: score = distinct agent names × 2 + log-ish hit buckets;
/// enough to rank hypotheses without a dedicated scoring model.
fn score_hits(resp: &Value) -> u64 {
    let total = resp
        .pointer("/hits/total/value")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let hosts = resp
        .pointer("/hits/hits")
        .and_then(Value::as_array)
        .map(|hits| {
            let mut hosts: Vec<&str> = hits
                .iter()
                .filter_map(|h| h.pointer("/_source/agent/name").and_then(Value::as_str))
                .collect();
            hosts.sort_unstable();
            hosts.dedup();
            hosts.len() as u64
        })
        .unwrap_or(0);
    let bucketed = match total {
        0 => 0,
        1..=2 => 1,
        3..=9 => 3,
        10..=49 => 6,
        _ => 8,
    };
    bucketed + hosts * 2
}

/// Opens a TheHive case for a scored hunt finding.
async fn open_finding(
    gateway: &HttpToolGateway,
    h: &hypotheses::Hypothesis,
    score: u64,
    resp: &Value,
    run_id: Uuid,
) -> anyhow::Result<()> {
    let case = NewCase {
        title: format!("[hunt] {} — {}", h.technique, h.name),
        description: format!(
            "{}\n\nHypothesis `{}` scored {} (threshold {CASE_THRESHOLD}).\n\nRaw hits (truncated):\n{}",
            h.description,
            h.id,
            score,
            serde_json::to_string_pretty(resp.pointer("/hits/hits").unwrap_or(&Value::Null))
                .unwrap_or_default()
                .chars()
                .take(4000)
                .collect::<String>()
        ),
        severity: if score >= 10 { 3 } else { 2 },
        tags: vec!["hunt".to_string(), format!("attack.{}", h.technique.to_ascii_lowercase())],
        tlp: 2,
    };
    let call = ToolCall {
        id: Uuid::new_v4(),
        tool: "thehive.case.create".to_string(),
        args: serde_json::to_value(&case)?,
        actor: "hunt-agent".to_string(),
        idempotency_key: format!("hunt-case:{}:{score}", h.id),
        correlation_id: run_id,
    };
    let created = gateway.invoke_allowed(&call).await?;
    info!(
        case_id = created.get("id").and_then(serde_json::Value::as_str).unwrap_or("?"),
        hunt = h.id,
        "hunt finding case opened"
    );
    Ok(())
}

/// Runs every hypothesis once; returns the number of findings opened.
async fn run_sweep(deps: &Deps, run_id: Uuid) -> anyhow::Result<usize> {
    if kill_switch_engaged() {
        warn!("kill switch engaged; hunt sweep halted");
        publish_activity(
            &deps.nats,
            &ActivityEvent::new(run_id, "hunt", "kill_switch", "sweep halted"),
        )
        .await;
        return Ok(0);
    }
    let mut findings = 0;
    for h in hypotheses::all() {
        publish_activity(
            &deps.nats,
            &ActivityEvent::new(
                run_id,
                "hunt",
                "run",
                format!("{} ({})", h.name, h.technique),
            ),
        )
        .await;
        let resp = match deps.os.query(HUNT_INDEX, "", (h.query)()).await {
            Ok(r) => r,
            Err(e) => {
                warn!(error = %e, hunt = h.id, "hunt query failed");
                continue;
            }
        };
        let score = score_hits(&resp);
        if score >= CASE_THRESHOLD {
            open_finding(&deps.gateway, &h, score, &resp, run_id).await?;
            findings += 1;
            publish_activity(
                &deps.nats,
                &ActivityEvent::new(
                    run_id,
                    "hunt",
                    "finding",
                    format!("{} scored {score}", h.name),
                ),
            )
            .await;
        } else {
            publish_activity(
                &deps.nats,
                &ActivityEvent::new(
                    run_id,
                    "hunt",
                    "clear",
                    format!("{} scored {score}", h.name),
                ),
            )
            .await;
        }
    }
    publish_activity(
        &deps.nats,
        &ActivityEvent::new(run_id, "hunt", "done", format!("{findings} findings")),
    )
    .await;
    Ok(findings)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let config = Config::parse();
    let nats = async_nats::connect(&config.nats_url).await?;
    let conn_cfg = ConnectorConfig::from_env();
    let deps = Arc::new(Deps {
        nats: nats.clone(),
        os: Arc::new(
            conn_cfg
                .build_opensearch_client()
                .context("OPENSEARCH_URL not set")?,
        ),
        gateway: Arc::new(HttpToolGateway::new(config.tool_server_url.clone())),
    });

    let mut sub = nats.subscribe(SUBJECT_HUNT.to_string()).await?;
    info!(
        subject = SUBJECT_HUNT,
        interval_s = config.hunt_interval_secs,
        "hunt-agent started"
    );

    let cron_deps = deps.clone();
    let interval_secs = config.hunt_interval_secs;
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(interval_secs));
        loop {
            interval.tick().await;
            let id = Uuid::new_v4();
            let span = tracing::info_span!("hunt", workflow_run_id = %id);
            match run_sweep(&cron_deps, id).instrument(span).await {
                Ok(n) => info!(findings = n, "hunt sweep complete"),
                Err(e) => error!(error = %e, "hunt sweep failed"),
            }
        }
    });

    while let Some(msg) = sub.next().await {
        let deps = deps.clone();
        tokio::spawn(async move {
            let result: anyhow::Result<()> = async {
                let envelope: Envelope =
                    serde_json::from_slice(&msg.payload).context("parse envelope")?;
                let span = tracing::info_span!(
                    "hunt",
                    workflow_run_id = %envelope.correlation_id,
                    envelope_id = %envelope.id,
                    kind = ?envelope.kind,
                );
                // NOTE: findings go to TheHive + ares.activity only. Publishing
                // back onto SUBJECT_HUNT would self-trigger an endless sweep.
                let _findings = run_sweep(&deps, envelope.correlation_id)
                    .instrument(span)
                    .await?;
                Ok(())
            }
            .await;
            if let Err(e) = result {
                error!(error = %e, "triggered hunt failed");
            }
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn score_rewards_distinct_hosts() {
        let one_host = json!({"hits": {"total": {"value": 5}, "hits": [
            {"_source": {"agent": {"name": "web-01"}}},
        ]}});
        let three_hosts = json!({"hits": {"total": {"value": 5}, "hits": [
            {"_source": {"agent": {"name": "web-01"}}},
            {"_source": {"agent": {"name": "web-02"}}},
            {"_source": {"agent": {"name": "web-03"}}},
        ]}});
        assert!(score_hits(&three_hosts) > score_hits(&one_host));
        assert_eq!(score_hits(&json!({})), 0);
    }
}
}
