//! NVD API 2.0 client (CVE lookup, windows, history).
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;
use tokio::time::sleep;

use anyhow::{Context, Result};

use reqwest::Client;

use serde::{Deserialize, Serialize};

use serde_json::Value;

use crate::http::join;

// ---------------------------------------------------------------------------
// NVD (National Vulnerability Database) API 2.0
// ---------------------------------------------------------------------------

const NVD_BASE: &str = "https://services.nvd.nist.gov";
/// Public rate limit: ~5 req/30s => one request every 6s.
const NVD_INTERVAL_ANON: Duration = Duration::from_millis(6_000);
/// With an API key: ~50 req/30s => one request every 600ms.
const NVD_INTERVAL_KEY: Duration = Duration::from_millis(600);
const NVD_MAX_RETRIES: u32 = 3;

/// NVD answered with a non-success status after retries.
#[derive(Debug)]
pub struct NvdStatusError(pub u16);

impl std::fmt::Display for NvdStatusError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "NVD HTTP {}", self.0)
    }
}

impl std::error::Error for NvdStatusError {}

/// One CVSS metric assessment as reported by NVD or a CNA.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CvssMetric {
    /// e.g. "4.0", "3.1", "2.0".
    pub version: String,
    /// Who supplied the score: `nvd@nist.gov` or the assigning CNA.
    pub source: String,
    /// `Primary` or `Secondary`.
    #[serde(rename = "type")]
    pub kind: String,
    pub vector: String,
    pub base_score: f64,
    #[serde(default)]
    pub severity: Option<String>,
}

/// A reference link on the NVD record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CveReference {
    pub url: String,
    #[serde(default)]
    pub tags: Vec<String>,
}

/// Normalized subset of an NVD CVE record; the full payload stays in `raw`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CveRecord {
    pub id: String,
    /// Assigning CNA (`sourceIdentifier`, e.g. `security@example.com`).
    #[serde(default)]
    pub source_identifier: Option<String>,
    #[serde(default)]
    pub published: Option<String>,
    #[serde(default)]
    pub last_modified: Option<String>,
    /// e.g. `Analyzed`, `Awaiting Analysis`, `Rejected`.
    #[serde(default)]
    pub vuln_status: Option<String>,
    /// English description text.
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub metrics: Vec<CvssMetric>,
    /// CWE identifiers (e.g. `CWE-79`).
    #[serde(default)]
    pub weaknesses: Vec<String>,
    /// CPE applicability tree, kept raw.
    #[serde(default)]
    pub configurations: Value,
    #[serde(default)]
    pub references: Vec<CveReference>,
    /// Whether the CVE is in CISA's KEV catalog (`cveTags`/`hasKev`).
    #[serde(default)]
    pub has_kev: bool,
    #[serde(skip_serializing_if = "Value::is_null", default)]
    pub raw: Value,
}

/// A page of CVE list results.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CvePage {
    pub total_results: u64,
    pub start_index: u64,
    pub results_per_page: u64,
    #[serde(default)]
    pub cves: Vec<CveRecord>,
}

/// NVD API 2.0 client with client-side rate limiting and retry.
///
/// Rate limits per NVD docs: ~5 requests per 30s rolling window without an
/// API key, ~50 per 30s with one. Requests are spaced via a shared mutex so
/// concurrent callers serialize instead of racing the limit.
#[derive(Clone)]
pub struct NvdClient {
    http: Client,
    api_key: Option<String>,
    min_interval: Duration,
    last_call: Arc<Mutex<Option<Instant>>>,
}

impl NvdClient {
    pub fn new(api_key: Option<String>) -> Self {
        let min_interval = if api_key.is_some() {
            NVD_INTERVAL_KEY
        } else {
            NVD_INTERVAL_ANON
        };
        Self {
            http: Client::new(),
            api_key,
            min_interval,
            last_call: Arc::new(Mutex::new(None)),
        }
    }

    /// Serialize requests and enforce the minimum inter-request interval.
    async fn throttle(&self) -> tokio::sync::MutexGuard<'_, Option<Instant>> {
        let mut last = self.last_call.lock().await;
        if let Some(t) = *last {
            let elapsed = t.elapsed();
            if elapsed < self.min_interval {
                sleep(self.min_interval - elapsed).await;
            }
        }
        *last = Some(Instant::now());
        last
    }

    /// GET `path` with query params, honoring the NVD rate limit and retrying
    /// transient failures (403/429/5xx) with exponential backoff.
    async fn get(&self, path: &str, query: &[(&str, String)]) -> Result<Value> {
        let mut attempt = 0u32;
        loop {
            let _guard = self.throttle().await;
            let mut req = self.http.get(join(NVD_BASE, path));
            if let Some(key) = &self.api_key {
                req = req.header("apiKey", key);
            }
            match req.query(query).send().await {
                Ok(resp) if resp.status().is_success() => {
                    return resp.json::<Value>().await.context("decode NVD JSON");
                }
                Ok(resp) => {
                    let status = resp.status();
                    let retryable = status.as_u16() == 403
                        || status.as_u16() == 429
                        || status.is_server_error();
                    if !retryable || attempt >= NVD_MAX_RETRIES {
                        return Err(anyhow::Error::new(NvdStatusError(status.as_u16())));
                    }
                }
                Err(e) => {
                    if attempt >= NVD_MAX_RETRIES {
                        return Err(e).context("NVD request failed");
                    }
                }
            }
            attempt += 1;
            sleep(Duration::from_secs(1 << attempt)).await;
        }
    }

    /// `GET /rest/json/cves/2.0?cveId=` — one CVE by ID, `None` when NVD has
    /// no record (unknown, reserved-and-unpublished, or rejected IDs may 404
    /// or return an empty list).
    pub async fn get_cve(&self, cve_id: &str) -> Result<Option<CveRecord>> {
        let raw = match self
            .get("/rest/json/cves/2.0", &[("cveId", cve_id.to_string())])
            .await
        {
            Ok(raw) => raw,
            // NVD answers 404 for ids it does not know; that is a miss, not an
            // outage, so callers can negative-cache it.
            Err(e) if e.downcast_ref::<NvdStatusError>().map(|s| s.0) == Some(404) => {
                return Ok(None);
            }
            Err(e) => return Err(e),
        };
        let Some(cve) = raw
            .get("vulnerabilities")
            .and_then(|v| v.as_array())
            .and_then(|a| a.first())
            .and_then(|v| v.get("cve"))
        else {
            return Ok(None);
        };
        Ok(Some(parse_cve(cve)))
    }

    /// `GET /rest/json/cves/2.0?pubStartDate&pubEndDate` — CVEs published in
    /// the (max 120-day) window, paginated via `start_index`.
    pub async fn list_published(
        &self,
        from: &str,
        to: &str,
        start_index: u64,
        extra: &[(&str, String)],
    ) -> Result<CvePage> {
        self.list_window("pubStartDate", "pubEndDate", from, to, start_index, extra)
            .await
    }

    /// `GET /rest/json/cves/2.0?lastModStartDate&lastModEndDate`.
    pub async fn list_modified(
        &self,
        from: &str,
        to: &str,
        start_index: u64,
        extra: &[(&str, String)],
    ) -> Result<CvePage> {
        self.list_window(
            "lastModStartDate",
            "lastModEndDate",
            from,
            to,
            start_index,
            extra,
        )
        .await
    }

    async fn list_window(
        &self,
        from_key: &str,
        to_key: &str,
        from: &str,
        to: &str,
        start_index: u64,
        extra: &[(&str, String)],
    ) -> Result<CvePage> {
        let mut q = vec![
            (from_key, from.to_string()),
            (to_key, to.to_string()),
            ("startIndex", start_index.to_string()),
            ("resultsPerPage", "2000".to_string()),
        ];
        q.extend(extra.iter().cloned());
        Ok(parse_cve_page(self.get("/rest/json/cves/2.0", &q).await?))
    }

    /// `GET /rest/json/cvehistory/2.0` — raw change-history events.
    pub async fn get_history(&self, cve_id: &str) -> Result<Value> {
        self.get(
            "/rest/json/cvehistory/2.0",
            &[("cveId", cve_id.to_string())],
        )
        .await
    }
}

/// Project one `cve` node of the NVD response into a [`CveRecord`].
fn parse_cve(cve: &Value) -> CveRecord {
    let description = cve["descriptions"]
        .as_array()
        .and_then(|ds| ds.iter().find(|d| d["lang"] == "en").or_else(|| ds.first()))
        .and_then(|d| d["value"].as_str())
        .map(str::to_string);

    let mut metrics = Vec::new();
    if let Some(m) = cve["metrics"].as_object() {
        for (_group, list) in m {
            for item in list.as_array().into_iter().flatten() {
                let data = &item["cvssData"];
                // Non-CVSS groups (e.g. ssvcV203) have empty cvssData — skip.
                if data["vectorString"].as_str().is_none() && data["baseScore"].as_f64().is_none() {
                    continue;
                }
                metrics.push(CvssMetric {
                    version: data["version"].as_str().unwrap_or("").to_string(),
                    source: item["source"].as_str().unwrap_or("").to_string(),
                    kind: item["type"].as_str().unwrap_or("").to_string(),
                    vector: data["vectorString"].as_str().unwrap_or("").to_string(),
                    base_score: data["baseScore"].as_f64().unwrap_or(0.0),
                    severity: data["baseSeverity"].as_str().map(str::to_string),
                });
            }
        }
    }

    let mut weaknesses = Vec::new();
    for w in cve["weaknesses"].as_array().into_iter().flatten() {
        for d in w["description"].as_array().into_iter().flatten() {
            if let Some(v) = d["value"].as_str() {
                if !weaknesses.iter().any(|x: &String| x == v) {
                    weaknesses.push(v.to_string());
                }
            }
        }
    }

    let references = cve["references"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|r| {
            Some(CveReference {
                url: r["url"].as_str()?.to_string(),
                tags: r["tags"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|t| t.as_str().map(str::to_string))
                    .collect(),
            })
        })
        .collect();

    let has_kev = cve["cveTags"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|t| t["tags"].as_array().into_iter().flatten())
        .any(|t| {
            t.as_str()
                .map(|s| s.eq_ignore_ascii_case("kev") || s.contains("Known Exploited"))
                .unwrap_or(false)
        })
        || cve["knownExploited"].as_bool().unwrap_or(false)
        || cve["cisaExploitAdd"].is_string();

    CveRecord {
        id: cve["id"].as_str().unwrap_or("").to_string(),
        source_identifier: cve["sourceIdentifier"].as_str().map(str::to_string),
        published: cve["published"].as_str().map(str::to_string),
        last_modified: cve["lastModified"].as_str().map(str::to_string),
        vuln_status: cve["vulnStatus"].as_str().map(str::to_string),
        description,
        metrics,
        weaknesses,
        configurations: cve["configurations"].clone(),
        references,
        has_kev,
        raw: cve.clone(),
    }
}

fn parse_cve_page(raw: Value) -> CvePage {
    let cves = raw["vulnerabilities"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|v| v.get("cve").map(parse_cve))
        .collect();
    CvePage {
        total_results: raw["totalResults"].as_u64().unwrap_or(0),
        start_index: raw["startIndex"].as_u64().unwrap_or(0),
        results_per_page: raw["resultsPerPage"].as_u64().unwrap_or(0),
        cves,
    }
}
