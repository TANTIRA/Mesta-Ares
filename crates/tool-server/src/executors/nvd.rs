//! NVD lookups and local CVSS scoring.

use std::sync::Arc;

use anyhow::{Context, Result};
use async_trait::async_trait;
use serde_json::{json, Value};

use super::{arg_str, dry_run, Tool};
use connectors::NvdClient;

/// `nvd.lookup_cve` / `nvd.cve_history` / `nvd.recent_cves` — NVD API 2.0.
/// The shared `NvdClient` serializes requests behind one rate limiter.
pub(crate) struct NvdTool {
    pub(crate) name: &'static str,
    pub(crate) client: Arc<NvdClient>,
}

#[async_trait]
impl Tool for NvdTool {
    fn name(&self) -> &'static str {
        self.name
    }
    async fn execute(&self, args: &Value) -> Result<Value> {
        match self.name {
            "nvd.lookup_cve" => {
                let cve_id = arg_str(args, &["cve_id"])
                    .context("missing cve_id")?
                    .to_uppercase();
                match self.client.get_cve(&cve_id).await? {
                    Some(rec) => Ok(serde_json::to_value(rec)?),
                    None => Ok(json!({ "found": false, "cve_id": cve_id })),
                }
            }
            "nvd.cve_history" => {
                let cve_id = arg_str(args, &["cve_id"])
                    .context("missing cve_id")?
                    .to_uppercase();
                self.client.get_history(&cve_id).await
            }
            "nvd.recent_cves" => {
                let days = args
                    .get("days")
                    .and_then(Value::as_u64)
                    .unwrap_or(1)
                    .clamp(1, 120);
                let to = chrono::Utc::now();
                let from = to - chrono::Duration::days(days as i64);
                let fmt = |t: chrono::DateTime<chrono::Utc>| {
                    t.format("%Y-%m-%dT%H:%M:%S%.3f").to_string()
                };
                let mut extra: Vec<(&str, String)> = Vec::new();
                if let Some(kw) = arg_str(args, &["keyword"]) {
                    extra.push(("keywordSearch", kw.to_string()));
                }
                for (arg, param) in [
                    ("cvss_v3_severity", "cvssV3Severity"),
                    ("cvss_v4_severity", "cvssV4Severity"),
                ] {
                    if let Some(sev) = arg_str(args, &[arg]) {
                        extra.push((param, sev.to_uppercase()));
                    }
                }
                if args
                    .get("has_kev")
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
                {
                    extra.push(("hasKev", String::new()));
                }
                let page = self
                    .client
                    .list_published(&fmt(from), &fmt(to), 0, &extra)
                    .await?;
                Ok(serde_json::to_value(page)?)
            }
            other => anyhow::bail!("unknown nvd executor `{other}`"),
        }
    }
}

/// `cvss.score` — local CVSS v3.1/v4.0 scoring via the `cvss` crate.
pub(crate) struct CvssScoreTool;

#[async_trait]
impl Tool for CvssScoreTool {
    fn name(&self) -> &'static str {
        "cvss.score"
    }
    async fn execute(&self, args: &Value) -> Result<Value> {
        let vector = arg_str(args, &["vector"]).context("missing vector")?;
        let score = cvss::parse_vector(vector).map_err(|e| anyhow::anyhow!("{e}"))?;
        Ok(serde_json::to_value(score)?)
    }
}

/// Stub executor for tools with no wired backend yet (and for the
/// `autonomy: deny` entries that policy rejects before dispatch anyway).
pub(crate) struct DryRun {
    pub(crate) name: &'static str,
}

#[async_trait]
impl Tool for DryRun {
    fn name(&self) -> &'static str {
        self.name
    }
    async fn execute(&self, args: &Value) -> Result<Value> {
        dry_run(args)
    }
}
