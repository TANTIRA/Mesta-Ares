//! Vulnerability-intelligence agent for the Ares pipeline.
//!
//! On `VULN_POLL_INTERVAL` it polls NVD API 2.0 for CVEs published in the
//! look-back window plus the CISA KEV flag, filters them against
//! `policies/vuln-watchlist.yml` (deployed-stack keywords) and a minimum
//! CVSS severity, then:
//! - persists new advisories to `vuln_advisories` (dedup by `cve_id`),
//! - emits progress to `ares.activity` for the console SSE feed,
//! - publishes the advisory to `ares.vulns`,
//! - opens a TheHive case for qualifying CVEs unless the kill switch is
//!   engaged.
//!
//! CNA-counting awareness: `source_identifier` (the assigning CNA) is stored
//! on every advisory so downstream reporting can attribute records.

use std::time::Duration;

use agent_core::{kill_switch_engaged, publish_activity};
use agent_protocol::activity::ActivityEvent;
use clap::Parser;
use connectors::{ConnectorConfig, CveRecord, NewCase, NvdClient};
use serde::Deserialize;
use serde_json::json;
use sqlx::PgPool;
use tracing::{error, info, warn};
use uuid::Uuid;

/// Subject advisories are published to.
const SUBJECT_VULNS: &str = "ares.vulns";

/// vuln-agent service configuration.
#[derive(Debug, Parser)]
#[command(name = "vuln-agent", about = "Ares NVD vulnerability watcher")]
struct Config {
    /// NATS server URL.
    #[arg(long, env = "NATS_URL", default_value = "nats://localhost:4222")]
    nats_url: String,

    /// Postgres connection URL.
    #[arg(long, env = "DATABASE_URL")]
    database_url: String,

    /// Seconds between NVD polls.
    #[arg(long, env = "VULN_POLL_INTERVAL", default_value = "3600")]
    poll_interval_secs: u64,

    /// Look-back window in hours for published-CVE queries.
    #[arg(long, env = "VULN_LOOKBACK_HOURS", default_value = "24")]
    lookback_hours: i64,

    /// Minimum base severity to escalate: low|medium|high|critical.
    #[arg(long, env = "VULN_MIN_SEVERITY", default_value = "high")]
    min_severity: String,

    /// Watchlist YAML path.
    #[arg(
        long,
        env = "VULN_WATCHLIST",
        default_value = "policies/vuln-watchlist.yml"
    )]
    watchlist_path: String,
}

/// `policies/vuln-watchlist.yml` — keyword filters for the deployed stack.
#[derive(Debug, Default, Deserialize)]
struct Watchlist {
    /// Product/vendor keywords matched (case-insensitive) against the CVE
    /// description, e.g. `opensearch`, `wazuh`, `thehive`.
    #[serde(default)]
    keywords: Vec<String>,
}

fn load_watchlist(path: &str) -> Watchlist {
    match std::fs::read_to_string(path) {
        Ok(raw) => serde_yaml_ng::from_str(&raw).unwrap_or_else(|e| {
            warn!(error = %e, path, "watchlist parse failed; using empty list");
            Watchlist::default()
        }),
        Err(e) => {
            warn!(error = %e, path, "watchlist unreadable; using empty list");
            Watchlist::default()
        }
    }
}

fn severity_rank(sev: &str) -> u8 {
    match sev.to_ascii_lowercase().as_str() {
        "low" => 1,
        "medium" => 2,
        "high" => 3,
        "critical" => 4,
        _ => 0,
    }
}

/// Highest base score across all reported CVSS metric versions.
fn best_cvss(rec: &CveRecord) -> Option<(f64, String)> {
    rec.metrics
        .iter()
        .filter(|m| m.base_score > 0.0)
        .max_by(|a, b| a.base_score.total_cmp(&b.base_score))
        .map(|m| {
            (
                m.base_score,
                m.severity.clone().unwrap_or_else(|| "UNKNOWN".into()),
            )
        })
}

/// First watchlist keyword found in the record, if any.
fn watchlist_match(rec: &CveRecord, wl: &Watchlist) -> Option<String> {
    let desc = rec.description.as_deref().unwrap_or("").to_lowercase();
    wl.keywords
        .iter()
        .find(|k| desc.contains(&k.to_lowercase()))
        .cloned()
}


/// Create the advisory table when absent.
async fn ensure_schema(pool: &PgPool) -> anyhow::Result<()> {
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS vuln_advisories (
             cve_id          text PRIMARY KEY,
             first_seen      timestamptz NOT NULL DEFAULT now(),
             last_checked    timestamptz NOT NULL DEFAULT now(),
             severity        text,
             score           float8,
             kev             boolean NOT NULL DEFAULT false,
             summary         text,
             watchlist_match text,
             cna             text,
             case_id         text,
             status          text NOT NULL DEFAULT 'new'
         )",
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// Insert the advisory; returns true when the row was new (dedup key:
/// `cve_id` — one CVE per vulnerability per CNA counting rules).
async fn record_advisory(
    pool: &PgPool,
    rec: &CveRecord,
    severity: &str,
    score: f64,
    matched: Option<&str>,
) -> anyhow::Result<bool> {
    let inserted = sqlx::query_scalar::<_, Option<String>>(
        "INSERT INTO vuln_advisories
             (cve_id, severity, score, kev, summary, watchlist_match, cna)
         VALUES ($1,$2,$3,$4,$5,$6,$7)
         ON CONFLICT (cve_id) DO UPDATE SET last_checked = now()
         RETURNING CASE WHEN xmax = 0 THEN cve_id ELSE NULL END",
    )
    .bind(&rec.id)
    .bind(severity)
    .bind(score)
    .bind(rec.has_kev)
    .bind(&rec.description)
    .bind(matched)
    .bind(&rec.source_identifier)
    .fetch_one(pool)
    .await?;
    Ok(inserted.is_some())
}

/// One poll cycle: NVD window query + KEV query, filter, persist, publish.
async fn poll_once(
    nvd: &NvdClient,
    pool: &PgPool,
    nats: &async_nats::Client,
    hive: Option<&connectors::ThehiveClient>,
    wl: &Watchlist,
    cfg: &Config,
) -> anyhow::Result<()> {
    let to = chrono::Utc::now();
    let from = to - chrono::Duration::hours(cfg.lookback_hours);
    let fmt = |t: chrono::DateTime<chrono::Utc>| t.format("%Y-%m-%dT%H:%M:%S%.3f").to_string();

    let mut seen: Vec<CveRecord> = nvd.list_published(&fmt(from), &fmt(to), 0, &[]).await?.cves;
    // KEV-flagged CVEs touched in the same window. NVD does not put a KEV
    // marker on the record itself — `hasKev` on the query is the marker, so
    // records returned here are KEV by definition.
    let kev = nvd
        .list_modified(
            &fmt(from),
            &fmt(to),
            0,
            &[("hasKev", String::new())],
        )
        .await
        .map(|p| p.cves)
        .unwrap_or_default();
    for mut rec in kev {
        rec.has_kev = true;
        if !seen.iter().any(|r| r.id == rec.id) {
            seen.push(rec);
        }
    }

    let min_rank = severity_rank(&cfg.min_severity);
    let mut escalated = 0usize;
    for rec in seen {
        let Some((score, severity)) = best_cvss(&rec) else {
            continue;
        };
        let matched = watchlist_match(&rec, wl);
        let qualifies = severity_rank(&severity) >= min_rank || rec.has_kev || matched.is_some();
        if !qualifies {
            continue;
        }
        if !record_advisory(pool, &rec, &severity, score, matched.as_deref()).await? {
            continue; // already known
        }
        escalated += 1;
        let advisory = json!({
            "cve_id": rec.id,
            "severity": severity,
            "score": score,
            "kev": rec.has_kev,
            "cna": rec.source_identifier,
            "watchlist_match": matched,
            "published": rec.published,
            "summary": rec.description,
        });
        if let Ok(bytes) = serde_json::to_vec(&advisory) {
            if let Err(e) = nats.publish(SUBJECT_VULNS.to_string(), bytes.into()).await {
                warn!(error = %e, "vulns publish failed");
            }
        }
        if let Some(hive) = hive {
            if kill_switch_engaged() {
                warn!(cve = %rec.id, "kill switch engaged; skipping case creation");
            } else {
                let case = NewCase {
                    title: format!("[NVD] {} — {}", rec.id, severity),
                    description: rec.description.clone().unwrap_or_default(),
                    severity: severity_rank(&severity),
                    tags: vec![
                        "ares".to_string(),
                        "vuln".to_string(),
                        if rec.has_kev {
                            "kev".into()
                        } else {
                            "nvd".into()
                        },
                    ],
                    tlp: 2,
                };
                match hive.create_case(&case).await {
                    Ok(created) => {
                        let _ = sqlx::query(
                            "UPDATE vuln_advisories SET case_id = $1, status = 'cased'
                             WHERE cve_id = $2",
                        )
                        .bind(&created.id)
                        .bind(&rec.id)
                        .execute(pool)
                        .await;
                    }
                    Err(e) => warn!(error = %e, cve = %rec.id, "thehive case failed"),
                }
            }
        }
    }
    publish_activity(
        nats,
        &ActivityEvent::new(
            Uuid::new_v4(),
            "vuln",
            "poll",
            format!("window={}h new_advisories={escalated}", cfg.lookback_hours),
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

    let cfg = Config::parse();
    let conn_cfg = ConnectorConfig::from_env();
    let nvd = conn_cfg.build_nvd_client();
    let hive = conn_cfg.build_thehive_client();
    let pool = PgPool::connect(&cfg.database_url).await?;
    ensure_schema(&pool).await?;
    let nats = async_nats::connect(&cfg.nats_url).await?;
    let wl = load_watchlist(&cfg.watchlist_path);
    info!(
        interval_secs = cfg.poll_interval_secs,
        min_severity = %cfg.min_severity,
        "vuln-agent started"
    );

    let mut interval = tokio::time::interval(Duration::from_secs(cfg.poll_interval_secs));
    loop {
        interval.tick().await;
        if let Err(e) = poll_once(&nvd, &pool, &nats, hive.as_ref(), &wl, &cfg).await {
            error!(error = %e, "vuln poll failed");
        }
    }
}
