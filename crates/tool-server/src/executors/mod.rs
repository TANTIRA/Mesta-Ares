//! Tool executors and the startup registry.
//!
//! Every entry in `policies/tools.yml` maps to one [`Tool`] implementation.
//! Executors holding a real backend make the connector call when the required
//! env vars exist; otherwise they return `{ "dry_run": true, args }` so the
//! pipeline stays exercisable without credentials.

mod openshell;

use std::collections::HashMap;
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use anyhow::{Context, Result};
use async_trait::async_trait;
use connectors::{
    ConnectorConfig, MispClient, NewAttribute, NewCase, NewObservable, NvdClient, OpensearchClient,
    ShuffleClient, ThehiveClient, WazuhClient,
};
use serde_json::{json, Value};

/// A dispatchable tool executor.
#[async_trait]
pub trait Tool: Send + Sync {
    /// Allowlist name this executor serves.
    fn name(&self) -> &'static str;
    /// Execute the (already policy-approved) call.
    async fn execute(&self, args: &Value) -> Result<Value>;
}

fn env(key: &str) -> Option<String> {
    std::env::var(key).ok().filter(|s| !s.is_empty())
}

fn dry_run(args: &Value) -> Result<Value> {
    Ok(json!({ "dry_run": true, "args": args }))
}

fn arg_str<'a>(args: &'a Value, keys: &[&str]) -> Option<&'a str> {
    keys.iter().find_map(|k| args.get(k)?.as_str())
}

/// Shared HTTP client for executors that hit provider APIs directly.
fn http() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new())
    })
}

fn b64url(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::with_capacity(data.len() * 4 / 3 + 4);
    for chunk in data.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | b[2] as u32;
        out.push(T[(n >> 18) as usize & 63] as char);
        out.push(T[(n >> 12) as usize & 63] as char);
        if chunk.len() > 1 {
            out.push(T[(n >> 6) as usize & 63] as char);
        }
        if chunk.len() > 2 {
            out.push(T[n as usize & 63] as char);
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Connector-backed executors
// ---------------------------------------------------------------------------

/// `opensearch.query` — runs a `_search` through [`OpensearchClient`].
struct OpensearchQuery(Option<OpensearchClient>);

#[async_trait]
impl Tool for OpensearchQuery {
    fn name(&self) -> &'static str {
        "opensearch.query"
    }
    async fn execute(&self, args: &Value) -> Result<Value> {
        let Some(c) = &self.0 else {
            return dry_run(args);
        };
        let index = arg_str(args, &["index"]).unwrap_or("_all");
        let q = arg_str(args, &["q", "query_string"]).unwrap_or("");
        let body = args
            .get("body")
            .cloned()
            .unwrap_or_else(|| json!({ "query": { "match_all": {} } }));
        c.query(index, q, body).await
    }
}

/// `thehive.case.create`.
struct ThehiveCase(Option<ThehiveClient>);

#[async_trait]
impl Tool for ThehiveCase {
    fn name(&self) -> &'static str {
        "thehive.case.create"
    }
    async fn execute(&self, args: &Value) -> Result<Value> {
        let Some(c) = &self.0 else {
            return dry_run(args);
        };
        let case: NewCase = serde_json::from_value(args.clone()).context("invalid NewCase args")?;
        Ok(serde_json::to_value(c.create_case(&case).await?)?)
    }
}

/// `thehive.case.update` — `{case_id, patch}`.
struct ThehiveCaseUpdate(Option<ThehiveClient>);

#[async_trait]
impl Tool for ThehiveCaseUpdate {
    fn name(&self) -> &'static str {
        "thehive.case.update"
    }
    async fn execute(&self, args: &Value) -> Result<Value> {
        let Some(c) = &self.0 else {
            return dry_run(args);
        };
        let case_id = arg_str(args, &["case_id", "id"]).context("missing case_id")?;
        let patch = args.get("patch").cloned().unwrap_or_else(|| args.clone());
        Ok(serde_json::to_value(c.update_case(case_id, &patch).await?)?)
    }
}

/// `thehive.observable.add` — `{case_id, observable:{...}}` or flat fields.
struct ThehiveObservable(Option<ThehiveClient>);

#[async_trait]
impl Tool for ThehiveObservable {
    fn name(&self) -> &'static str {
        "thehive.observable.add"
    }
    async fn execute(&self, args: &Value) -> Result<Value> {
        let Some(c) = &self.0 else {
            return dry_run(args);
        };
        let case_id = arg_str(args, &["case_id", "id"]).context("missing case_id")?;
        let obs_val = args
            .get("observable")
            .cloned()
            .unwrap_or_else(|| args.clone());
        let obs: NewObservable =
            serde_json::from_value(obs_val).context("invalid NewObservable args")?;
        Ok(serde_json::to_value(
            c.add_observable(case_id, &obs).await?,
        )?)
    }
}

/// `cortex.analyzer.run` — `{analyzer_id, data_type, data}`.
struct CortexAnalyzer(Option<connectors::CortexClient>);

#[async_trait]
impl Tool for CortexAnalyzer {
    fn name(&self) -> &'static str {
        "cortex.analyzer.run"
    }
    async fn execute(&self, args: &Value) -> Result<Value> {
        let Some(c) = &self.0 else {
            return dry_run(args);
        };
        let analyzer_id =
            arg_str(args, &["analyzer_id", "analyzer"]).context("missing analyzer_id")?;
        let data_type = arg_str(args, &["data_type", "dataType"]).context("missing data_type")?;
        let data = arg_str(args, &["data", "observable"]).context("missing data")?;
        c.run_analyzer(analyzer_id, data_type, data).await
    }
}

/// `misp.search` — `{value}`.
struct MispSearch(Option<MispClient>);

#[async_trait]
impl Tool for MispSearch {
    fn name(&self) -> &'static str {
        "misp.search"
    }
    async fn execute(&self, args: &Value) -> Result<Value> {
        let Some(c) = &self.0 else {
            return dry_run(args);
        };
        let value = arg_str(args, &["value", "ioc", "query"]).context("missing value")?;
        Ok(serde_json::to_value(c.search(value).await?)?)
    }
}

/// `misp.attribute.add` and `misp.ioc.block` — `{event_id?, attribute|...}`.
/// `event_id` falls back to `MISP_BLOCK_EVENT_ID`.
struct MispAttribute {
    name: &'static str,
    client: Option<MispClient>,
}

#[async_trait]
impl Tool for MispAttribute {
    fn name(&self) -> &'static str {
        self.name
    }
    async fn execute(&self, args: &Value) -> Result<Value> {
        let Some(c) = &self.client else {
            return dry_run(args);
        };
        let event_id = arg_str(args, &["event_id"])
            .map(str::to_string)
            .or_else(|| env("MISP_BLOCK_EVENT_ID"))
            .context("missing event_id (and MISP_BLOCK_EVENT_ID unset)")?;
        let attr_val = args
            .get("attribute")
            .cloned()
            .unwrap_or_else(|| args.clone());
        let attr: NewAttribute =
            serde_json::from_value(attr_val).context("invalid NewAttribute args")?;
        c.add_attribute(&event_id, &attr).await
    }
}

/// `misp.attribute.search` — `{value, limit?}`.
struct MispAttributeSearch(Option<MispClient>);

#[async_trait]
impl Tool for MispAttributeSearch {
    fn name(&self) -> &'static str {
        "misp.attribute.search"
    }
    async fn execute(&self, args: &Value) -> Result<Value> {
        let Some(c) = &self.0 else {
            return dry_run(args);
        };
        let value = arg_str(args, &["value", "ioc", "query"]).context("missing value")?;
        let limit = args.get("limit").and_then(Value::as_u64).unwrap_or(50) as u32;
        Ok(serde_json::to_value(
            c.search_attributes(value, limit).await?,
        )?)
    }
}

/// `misp.event.view` — `{event_id}`.
struct MispEventView(Option<MispClient>);

#[async_trait]
impl Tool for MispEventView {
    fn name(&self) -> &'static str {
        "misp.event.view"
    }
    async fn execute(&self, args: &Value) -> Result<Value> {
        let Some(c) = &self.0 else {
            return dry_run(args);
        };
        let event_id = arg_str(args, &["event_id", "id"]).context("missing event_id")?;
        c.event(event_id).await
    }
}

/// `misp.warninglist.check` — `{values: [..]}` or `{value}`.
struct MispWarninglist(Option<MispClient>);

#[async_trait]
impl Tool for MispWarninglist {
    fn name(&self) -> &'static str {
        "misp.warninglist.check"
    }
    async fn execute(&self, args: &Value) -> Result<Value> {
        let Some(c) = &self.0 else {
            return dry_run(args);
        };
        let values: Vec<String> = match args.get("values").and_then(Value::as_array) {
            Some(arr) => arr
                .iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect(),
            None => vec![arg_str(args, &["value", "ioc"])
                .context("missing values")?
                .to_string()],
        };
        Ok(serde_json::json!({ "matched": c.check_warninglists(&values).await? }))
    }
}

/// `misp.sighting.add` — `{values: [..], source?}`.
struct MispSighting(Option<MispClient>);

#[async_trait]
impl Tool for MispSighting {
    fn name(&self) -> &'static str {
        "misp.sighting.add"
    }
    async fn execute(&self, args: &Value) -> Result<Value> {
        let Some(c) = &self.0 else {
            return dry_run(args);
        };
        let values: Vec<String> = match args.get("values").and_then(Value::as_array) {
            Some(arr) => arr
                .iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect(),
            None => vec![arg_str(args, &["value", "ioc"])
                .context("missing values")?
                .to_string()],
        };
        let source = arg_str(args, &["source"])
            .map(str::to_string)
            .or_else(|| env("MISP_SIGHTING_SOURCE"));
        c.add_sightings(&values, source.as_deref()).await
    }
}

/// `wazuh.agent.list`.
struct WazuhAgents(Option<WazuhClient>);

#[async_trait]
impl Tool for WazuhAgents {
    fn name(&self) -> &'static str {
        "wazuh.agent.list"
    }
    async fn execute(&self, _args: &Value) -> Result<Value> {
        let Some(c) = &self.0 else {
            return dry_run(_args);
        };
        c.list_agents().await
    }
}

/// `wazuh.active_response.isolate_host` / `kill_process` — Wazuh AR commands.
struct WazuhActiveResponse {
    name: &'static str,
    command: &'static str,
    client: Option<WazuhClient>,
}

#[async_trait]
impl Tool for WazuhActiveResponse {
    fn name(&self) -> &'static str {
        self.name
    }
    async fn execute(&self, args: &Value) -> Result<Value> {
        let Some(c) = &self.client else {
            return dry_run(args);
        };
        let agent_id = arg_str(args, &["agent_id"]).context("missing agent_id")?;
        let mut ar_args: Vec<String> = Vec::new();
        if let Some(d) = args.get("duration_minutes").and_then(|v| v.as_i64()) {
            ar_args.push(d.to_string());
        }
        if let Some(p) = args.get("pid").and_then(|v| v.as_i64()) {
            ar_args.push(p.to_string());
        }
        c.active_response(agent_id, self.command, ar_args).await
    }
}

/// Shuffle-workflow-backed executor (`firewall.rule.add`,
/// `blocklist.ip.add`, `blocklist.domain.add`). Workflow id comes from the
/// per-tool env var, falling back to `SHUFFLE_FW_WORKFLOW_ID`.
struct ShuffleWorkflow {
    name: &'static str,
    env_key: &'static str,
    client: Option<ShuffleClient>,
}

#[async_trait]
impl Tool for ShuffleWorkflow {
    fn name(&self) -> &'static str {
        self.name
    }
    async fn execute(&self, args: &Value) -> Result<Value> {
        let (Some(c), Some(wf)) = (
            &self.client,
            env(self.env_key).or_else(|| env("SHUFFLE_FW_WORKFLOW_ID")),
        ) else {
            return dry_run(args);
        };
        c.run_workflow(&wf, args.clone()).await
    }
}

// ---------------------------------------------------------------------------
// Direct-API executors (no dedicated connector)
// ---------------------------------------------------------------------------

/// Threat-intel lookups: `virustotal.lookup`, `abuseipdb.check`,
/// `shodan.host`, `greynoise.ip`, `urlscan.submit`.
struct ProviderLookup {
    name: &'static str,
}

#[async_trait]
impl Tool for ProviderLookup {
    fn name(&self) -> &'static str {
        self.name
    }
    async fn execute(&self, args: &Value) -> Result<Value> {
        let ioc = arg_str(args, &["ioc", "value", "ip", "domain", "hash", "url"])
            .context("missing ioc/value arg")?;
        match self.name {
            "virustotal.lookup" => {
                let Some(key) = env("VIRUSTOTAL_API_KEY") else {
                    return dry_run(args);
                };
                let path = if ioc.parse::<std::net::IpAddr>().is_ok() {
                    format!("ip_addresses/{ioc}")
                } else if ioc.starts_with("http://") || ioc.starts_with("https://") {
                    format!("urls/{}", b64url(ioc.as_bytes()))
                } else if ioc.len() == 64 || ioc.len() == 32 {
                    format!("files/{ioc}")
                } else {
                    format!("domains/{ioc}")
                };
                Ok(http()
                    .get(format!("https://www.virustotal.com/api/v3/{path}"))
                    .header("x-apikey", key)
                    .send()
                    .await
                    .context("virustotal request")?
                    .error_for_status()
                    .context("virustotal HTTP error")?
                    .json::<Value>()
                    .await?)
            }
            "abuseipdb.check" => {
                let Some(key) = env("ABUSEIPDB_API_KEY") else {
                    return dry_run(args);
                };
                Ok(http()
                    .get("https://api.abuseipdb.com/api/v2/check")
                    .header("Key", key)
                    .header("Accept", "application/json")
                    .query(&[("ipAddress", ioc), ("maxAgeInDays", "90")])
                    .send()
                    .await
                    .context("abuseipdb request")?
                    .error_for_status()
                    .context("abuseipdb HTTP error")?
                    .json::<Value>()
                    .await?)
            }
            "shodan.host" => {
                let Some(key) = env("SHODAN_API_KEY") else {
                    return dry_run(args);
                };
                Ok(http()
                    .get(format!("https://api.shodan.io/shodan/host/{ioc}"))
                    .query(&[("key", key.as_str())])
                    .send()
                    .await
                    .context("shodan request")?
                    .error_for_status()
                    .context("shodan HTTP error")?
                    .json::<Value>()
                    .await?)
            }
            "greynoise.ip" => {
                let Some(key) = env("GREYNOISE_API_KEY") else {
                    return dry_run(args);
                };
                Ok(http()
                    .get(format!("https://api.greynoise.io/v3/community/{ioc}"))
                    .header("key", key)
                    .send()
                    .await
                    .context("greynoise request")?
                    .error_for_status()
                    .context("greynoise HTTP error")?
                    .json::<Value>()
                    .await?)
            }
            "urlscan.submit" => {
                let Some(key) = env("URLSCAN_API_KEY") else {
                    return dry_run(args);
                };
                Ok(http()
                    .post("https://urlscan.io/api/v1/scan/")
                    .header("API-Key", key)
                    .json(&json!({ "url": ioc, "visibility": "private" }))
                    .send()
                    .await
                    .context("urlscan request")?
                    .error_for_status()
                    .context("urlscan HTTP error")?
                    .json::<Value>()
                    .await?)
            }
            other => anyhow::bail!("unknown provider executor `{other}`"),
        }
    }
}

/// `entra.user.disable` / `entra.user.revoke_sessions` via Microsoft Graph.
/// Real call requires `ENTRA_BEARER_TOKEN`; otherwise dry-run.
struct EntraUser {
    name: &'static str,
}

#[async_trait]
impl Tool for EntraUser {
    fn name(&self) -> &'static str {
        self.name
    }
    async fn execute(&self, args: &Value) -> Result<Value> {
        let Some(token) = env("ENTRA_BEARER_TOKEN").or_else(|| env("GRAPH_BEARER_TOKEN")) else {
            return dry_run(args);
        };
        let user_id = arg_str(args, &["user_id", "id"]).context("missing user_id")?;
        let req = match self.name {
            "entra.user.disable" => http()
                .patch(format!("https://graph.microsoft.com/v1.0/users/{user_id}"))
                .json(&json!({ "accountEnabled": false })),
            "entra.user.revoke_sessions" => http().post(format!(
                "https://graph.microsoft.com/v1.0/users/{user_id}/revokeSignInSessions"
            )),
            other => anyhow::bail!("unknown entra executor `{other}`"),
        };
        Ok(req
            .bearer_auth(token)
            .send()
            .await
            .context("graph request")?
            .error_for_status()
            .context("graph HTTP error")?
            .json::<Value>()
            .await
            .unwrap_or_else(|_| Value::Null))
    }
}

/// `notify.slack.post` — posts to `SLACK_WEBHOOK_URL` if configured.
struct SlackNotify;

#[async_trait]
impl Tool for SlackNotify {
    fn name(&self) -> &'static str {
        "notify.slack.post"
    }
    async fn execute(&self, args: &Value) -> Result<Value> {
        let Some(webhook) = env("SLACK_WEBHOOK_URL") else {
            return dry_run(args);
        };
        let body = if args.get("text").is_some() || args.get("blocks").is_some() {
            args.clone()
        } else {
            json!({ "text": args.get("message").cloned().unwrap_or_else(|| args.clone()) })
        };
        Ok(http()
            .post(webhook)
            .json(&body)
            .send()
            .await
            .context("slack webhook")?
            .error_for_status()
            .context("slack HTTP error")?
            .text()
            .await
            .map(|t| json!({ "response": t }))?)
    }
}

/// `nvd.lookup_cve` / `nvd.cve_history` / `nvd.recent_cves` — NVD API 2.0.
/// The shared `NvdClient` serializes requests behind one rate limiter.
struct NvdTool {
    name: &'static str,
    client: Arc<NvdClient>,
}

#[async_trait]
impl Tool for NvdTool {
    fn name(&self) -> &'static str {
        self.name
    }
    async fn execute(&self, args: &Value) -> Result<Value> {
        match self.name {
            "nvd.lookup_cve" => {
                let cve_id = arg_str(args, &["cve_id", "id", "cve"])
                    .context("missing cve_id")?
                    .to_uppercase();
                match self.client.get_cve(&cve_id).await? {
                    Some(rec) => Ok(serde_json::to_value(rec)?),
                    None => Ok(json!({ "found": false, "cve_id": cve_id })),
                }
            }
            "nvd.cve_history" => {
                let cve_id = arg_str(args, &["cve_id", "id", "cve"])
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
                if let Some(kw) = arg_str(args, &["keyword", "q"]) {
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
struct CvssScoreTool;

#[async_trait]
impl Tool for CvssScoreTool {
    fn name(&self) -> &'static str {
        "cvss.score"
    }
    async fn execute(&self, args: &Value) -> Result<Value> {
        let vector = arg_str(args, &["vector", "cvss_vector"]).context("missing vector")?;
        let score = cvss::parse_vector(vector).map_err(|e| anyhow::anyhow!("{e}"))?;
        Ok(serde_json::to_value(score)?)
    }
}

/// Stub executor for tools with no wired backend yet (and for the
/// `autonomy: deny` entries that policy rejects before dispatch anyway).
struct DryRun {
    name: &'static str,
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

// ---------------------------------------------------------------------------
// Registry
// ---------------------------------------------------------------------------

/// Build the tool registry covering every name in `policies/tools.yml`.
pub fn build_registry(cfg: &ConnectorConfig) -> HashMap<String, Arc<dyn Tool>> {
    let mut reg: HashMap<String, Arc<dyn Tool>> = HashMap::new();
    let mut put = |t: Arc<dyn Tool>| {
        reg.insert(t.name().to_string(), t);
    };

    // One client (and connection pool) per backend, cloned into executors.
    let os = cfg.build_opensearch_client();
    let hive = cfg.build_thehive_client();
    let cortex = cfg.build_cortex_client();
    let misp = cfg.build_misp_client();
    let shuffle = cfg.build_shuffle_client();
    let wazuh = cfg.build_wazuh_client();

    // Read-only / enrichment
    put(Arc::new(OpensearchQuery(os)));
    put(Arc::new(ThehiveCase(hive.clone())));
    put(Arc::new(ThehiveCaseUpdate(hive.clone())));
    put(Arc::new(ThehiveObservable(hive)));
    put(Arc::new(CortexAnalyzer(cortex)));
    put(Arc::new(MispSearch(misp.clone())));
    put(Arc::new(MispAttribute {
        name: "misp.attribute.add",
        client: misp.clone(),
    }));
    put(Arc::new(MispAttribute {
        name: "misp.ioc.block",
        client: misp.clone(),
    }));
    put(Arc::new(MispAttributeSearch(misp.clone())));
    put(Arc::new(MispEventView(misp.clone())));
    put(Arc::new(MispWarninglist(misp.clone())));
    put(Arc::new(MispSighting(misp)));
    for name in [
        "virustotal.lookup",
        "abuseipdb.check",
        "shodan.host",
        "greynoise.ip",
        "urlscan.submit",
    ] {
        put(Arc::new(ProviderLookup { name }));
    }
    put(Arc::new(DryRun {
        name: "osquery.fleet_query",
    }));
    put(Arc::new(WazuhAgents(wazuh.clone())));

    // NVD / CVSS
    let nvd = Arc::new(cfg.build_nvd_client());
    for name in ["nvd.lookup_cve", "nvd.cve_history", "nvd.recent_cves"] {
        put(Arc::new(NvdTool {
            name,
            client: nvd.clone(),
        }));
    }
    put(Arc::new(CvssScoreTool));

    // Response actions
    put(Arc::new(ShuffleWorkflow {
        name: "blocklist.ip.add",
        env_key: "SHUFFLE_BLOCKLIST_WORKFLOW_ID",
        client: shuffle.clone(),
    }));
    put(Arc::new(ShuffleWorkflow {
        name: "blocklist.domain.add",
        env_key: "SHUFFLE_DOMAIN_WORKFLOW_ID",
        client: shuffle.clone(),
    }));
    put(Arc::new(ShuffleWorkflow {
        name: "firewall.rule.add",
        env_key: "SHUFFLE_FW_WORKFLOW_ID",
        client: shuffle.clone(),
    }));
    put(Arc::new(WazuhActiveResponse {
        name: "wazuh.active_response.isolate_host",
        command: "isolate-host",
        client: wazuh.clone(),
    }));
    put(Arc::new(WazuhActiveResponse {
        name: "wazuh.active_response.kill_process",
        command: "kill-process",
        client: wazuh.clone(),
    }));
    put(Arc::new(EntraUser {
        name: "entra.user.revoke_sessions",
    }));
    put(Arc::new(EntraUser {
        name: "entra.user.disable",
    }));
    put(Arc::new(DryRun {
        name: "ad.user.disable",
    }));
    put(Arc::new(DryRun {
        name: "o365.email.quarantine",
    }));

    // Detection-as-code + notifications
    put(Arc::new(DryRun {
        name: "detection.sigma.deploy",
    }));
    put(Arc::new(SlackNotify));

    // Dynamic command execution — always through the OpenShell sandbox (see
    // openshell.rs). `exec.bas.run` is registered ahead of its allowlist
    // entry so BAS local-exec lands in the sandbox the day policy enables it.
    put(Arc::new(openshell::OpenShellExec {
        name: "exec.sandbox",
    }));
    put(Arc::new(openshell::OpenShellExec {
        name: "exec.bas.run",
    }));

    // Explicitly denied by policy; registered so the registry covers the
    // whole allowlist file. Dispatch is never reached for these.
    for name in ["host.shutdown", "user.delete", "siem.index.delete"] {
        put(Arc::new(DryRun { name }));
    }

    reg
}

/// Names in the allowlist with no registered executor (startup sanity check).
pub fn missing_executors(
    registry: &HashMap<String, Arc<dyn Tool>>,
    allowlist: &[String],
) -> Vec<String> {
    allowlist
        .iter()
        .filter(|t| !registry.contains_key(*t))
        .cloned()
        .collect()
}

#[cfg(test)]
mod registry_tests {
    use super::*;

    /// Every tool named in `policies/tools.yml` must have an executor, or
    /// startup aborts (`missing_executors` in main).
    #[test]
    fn registry_covers_policy_tools() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../policies");
        let engine = policy_engine::PolicyEngine::load(&dir).expect("load policies");
        let reg = build_registry(&ConnectorConfig::from_env());
        assert!(
            missing_executors(&reg, &engine.tool_names()).is_empty(),
            "policy tools without executors: {:?}",
            missing_executors(&reg, &engine.tool_names())
        );
    }
}
