//! BAS runner for the Ares SIEM+SOAR pipeline.
//!
//! Loads scenario manifests from `bas/scenarios/*.yml` (another worker owns
//! the manifests; this binary only globs the directory), executes each
//! scenario's command on the test fleet, then measures:
//! - **MTTD** — time until a matching alert appears in OpenSearch
//!   (polls every 10 s for up to 10 min).
//! - **MTTR** — time until a containment action lands in `audit_log`.
//!
//! Each technique produces one `bas_results` row; every scenario publishes a
//! [`EnvelopeKind::BasResult`] envelope to [`SUBJECT_BAS`]; the orchestrator
//! routes undetected scenarios to [`SUBJECT_DETECTION`] so the detection agent
//! can fill the coverage gap.
//!
//! Execution transports: `BAS_SSH_TARGET` → `ssh <target> <command>`,
//! `BAS_WINRM_TARGET` → `winrm -r <target> <command>`. When the matching env
//! var is unset (or `--dry` is passed) the run is a logged dry run.
//!
//! NOTE: `bas_results` carries an `mttd_secs` column alongside the spec'd
//! `mttr_secs`/`mttr_contained`.

mod scenario;

use std::process::Command;
use std::time::{Duration, Instant};

use agent_protocol::envelope::{Envelope, EnvelopeKind};
use agent_protocol::SUBJECT_BAS;
use anyhow::Context;
use clap::Parser;
use connectors::{ConnectorConfig, OpensearchClient};
use serde_json::{json, Value};
use sqlx::PgPool;
use tracing::{error, info, warn};
use uuid::Uuid;

use crate::scenario::{Scenario, SCENARIOS_DIR};

/// Alert index pattern polled for detections.
const ALERT_INDEX: &str = "wazuh-alerts-*";
/// Poll cadence for detection/containment waits.
const POLL_INTERVAL: Duration = Duration::from_secs(10);
/// Max wait for detection and containment signals.
const POLL_TIMEOUT: Duration = Duration::from_secs(600);
/// Tools counted as containment signals in `audit_log`.
const CONTAINMENT_TOOLS: &[&str] = &[
    "wazuh.active_response.isolate_host",
    "blocklist.ip.add",
    "blocklist.domain.add",
    "firewall.rule.add",
    "entra.user.disable",
    "ad.user.disable",
    "entra.user.revoke_sessions",
];

/// BAS runner configuration.
#[derive(Debug, Parser)]
#[command(
    name = "bas-runner",
    about = "Ares breach-and-attack simulation runner"
)]
struct Config {
    /// NATS server URL.
    #[arg(long, env = "NATS_URL", default_value = "nats://localhost:4222")]
    nats_url: String,

    /// Postgres connection URL for `bas_results` and `audit_log`.
    #[arg(long, env = "DATABASE_URL")]
    database_url: String,

    /// Log the commands without executing them (also forced when the
    /// transport env var for a platform is unset).
    #[arg(long, env = "BAS_DRY_RUN", default_value = "false")]
    dry: bool,
}

/// Shared dependencies.
struct Deps {
    nats: async_nats::Client,
    os: OpensearchClient,
    pool: PgPool,
    dry: bool,
}

/// Runs the scenario command on the fleet, or logs a dry run.
fn execute(scenario: &Scenario, dry: bool) -> anyhow::Result<()> {
    let (env_var, program, args): (&str, &str, Vec<String>) =
        match scenario.executor.platform.as_str() {
            "winrm" | "windows" => ("BAS_WINRM_TARGET", "winrm", vec!["-r".to_string()]),
            _ => ("BAS_SSH_TARGET", "ssh", Vec::new()),
        };
    let target = std::env::var(env_var).ok();
    let target = match (target, dry) {
        (Some(t), false) => t,
        _ => {
            info!(scenario = %scenario.id, platform = %scenario.executor.platform,
                  cmd = %scenario.executor.command, "dry run (no {env_var} or --dry)");
            return Ok(());
        }
    };
    let mut full_args = args;
    full_args.push(target.clone());
    full_args.push(scenario.executor.command.clone());
    let status = Command::new(program)
        .args(&full_args)
        .status()
        .with_context(|| format!("spawn {program} for {}", scenario.id))?;
    anyhow::ensure!(
        status.success(),
        "{program} exited {status} for {}",
        scenario.id
    );
    Ok(())
}

/// Waits for an alert matching `technique` newer than `since`; returns MTTD.
async fn wait_mttd(
    os: &OpensearchClient,
    technique: &str,
    since: chrono::DateTime<chrono::Utc>,
) -> anyhow::Result<Option<u64>> {
    let started = Instant::now();
    let body = json!({
        "size": 1,
        "query": {"bool": {
            "must": [{"query_string": {"query": format!("*{technique}*")}}],
            "filter": [{"range": {"@timestamp": {"gte": since.to_rfc3339()}}}]
        }}
    });
    let mut last_error = None;
    while started.elapsed() < POLL_TIMEOUT {
        match os.query(ALERT_INDEX, "", body.clone()).await {
            Ok(resp) => {
                let hits = resp
                    .pointer("/hits/total/value")
                    .and_then(Value::as_u64)
                    .unwrap_or(0);
                if hits > 0 {
                    return Ok(Some(started.elapsed().as_secs()));
                }
            }
            Err(e) => {
                warn!(error = %e, technique, "mttd poll failed");
                last_error = Some(e);
            }
        }
        tokio::time::sleep(POLL_INTERVAL).await;
    }
    match last_error {
        // Only a clean timeout means "not detected"; a failing store must not
        // be recorded as a detection gap.
        None => Ok(None),
        Some(e) => Err(e.context("mttd poll never succeeded")),
    }
}

/// Waits for a containment signal in `audit_log` newer than `since`;
/// returns `(mttr_secs, contained)`.
async fn wait_mttr(
    pool: &PgPool,
    since: chrono::DateTime<chrono::Utc>,
) -> anyhow::Result<(Option<i64>, bool)> {
    let started = Instant::now();
    while started.elapsed() < POLL_TIMEOUT {
        let found = sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM audit_log
             WHERE decision = 'allow' AND ts > $1 AND tool = ANY($2)",
        )
        .bind(since)
        .bind(CONTAINMENT_TOOLS)
        .fetch_one(pool)
        .await?;
        if found > 0 {
            return Ok((Some(started.elapsed().as_secs() as i64), true));
        }
        tokio::time::sleep(POLL_INTERVAL).await;
    }
    Ok((None, false))
}

/// Publishes a JSON payload as an [`Envelope`] on `subject`.
async fn publish(nats: &async_nats::Client, subject: &str, kind: EnvelopeKind, payload: Value) {
    let env = Envelope::new("bas-runner", kind, payload);
    match serde_json::to_vec(&env) {
        Ok(bytes) => {
            if let Err(e) = nats.publish(subject.to_string(), bytes.into()).await {
                warn!(error = %e, subject, "publish failed");
            }
        }
        Err(e) => warn!(error = %e, "envelope serialize failed"),
    }
}

/// Runs one scenario end-to-end: execute, MTTD, MTTR, rows, envelopes.
async fn run_scenario(deps: &Deps, scenario: &Scenario) -> anyhow::Result<()> {
    info!(scenario = %scenario.id, name = %scenario.name, "running scenario");
    if deps.dry {
        // A dry run simulates the attack only: recording MTTD/MTTR would
        // poison the metrics table and republishing would trigger
        // detection-agent for a scenario that never ran.
        info!(scenario = %scenario.id, "dry run: skipping metrics and publish");
        return Ok(());
    }
    execute(scenario, deps.dry)?;
    let since = chrono::Utc::now();
    let started = Instant::now();

    // MTTD is measured against the first technique; every technique gets a
    // row sharing the scenario-level detection verdict.
    let primary = scenario
        .techniques
        .first()
        .cloned()
        .unwrap_or_else(|| "unknown".to_string());
    let mttd = wait_mttd(&deps.os, &primary, since).await?;
    let detected = mttd.is_some();

    // Only wait for containment when something was detected.
    let (mttr_secs, mttr_contained) = if detected {
        wait_mttr(&deps.pool, since).await?
    } else {
        (None, false)
    };

    for technique in &scenario.techniques {
        sqlx::query(
            "INSERT INTO bas_results (id, scenario_id, technique, detected, mttd_secs, mttr_secs, mttr_contained, ts)
             VALUES ($1, $2, $3, $4, $5, $6, $7, now())",
        )
        .bind(Uuid::new_v4())
        .bind(&scenario.id)
        .bind(technique)
        .bind(detected)
        .bind(mttd.map(|v| v as i64))
        .bind(mttr_secs)
        .bind(mttr_contained)
        .execute(&deps.pool)
        .await?;
    }

    let payload = json!({
        "scenario_id": scenario.id,
        "name": scenario.name,
        "target": scenario.target,
        "techniques": scenario.techniques,
        "detected": detected,
        "mttd_secs": mttd,
        "mttr_secs": mttr_secs,
        "mttr_contained": mttr_contained,
        "elapsed_secs": started.elapsed().as_secs(),
    });
    publish(
        &deps.nats,
        SUBJECT_BAS,
        EnvelopeKind::BasResult,
        payload.clone(),
    )
    .await;
    // The orchestrator routes undetected BasResult envelopes on to
    // detection-agent; publishing here as well would run it twice.
    Ok(())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let config = Config::parse();
    let scenarios = scenario::load_all()?;
    info!(
        count = scenarios.len(),
        dir = SCENARIOS_DIR,
        "scenarios loaded"
    );

    let nats = async_nats::connect(&config.nats_url).await?;
    let pool = PgPool::connect(&config.database_url).await?;
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS bas_results (
            id UUID PRIMARY KEY,
            scenario_id TEXT NOT NULL,
            technique TEXT NOT NULL,
            detected BOOLEAN NOT NULL,
            mttd_secs BIGINT,
            mttr_secs BIGINT,
            mttr_contained BOOLEAN NOT NULL DEFAULT false,
            ts TIMESTAMPTZ NOT NULL DEFAULT now()
        )",
    )
    .execute(&pool)
    .await?;

    let conn_cfg = ConnectorConfig::from_env();
    let deps = Deps {
        nats,
        os: conn_cfg
            .build_opensearch_client()
            .context("OPENSEARCH_URL not set")?,
        pool,
        dry: config.dry,
    };

    for (_, scenario) in &scenarios {
        if let Err(e) = run_scenario(&deps, scenario).await {
            error!(error = %e, scenario = %scenario.id, "scenario failed");
        }
    }
    info!("bas run complete");
    Ok(())
}
