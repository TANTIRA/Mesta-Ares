//! Thin async HTTP clients for the security stack's external systems.
//!
//! Each client wraps one upstream API behind `new(base_url, auth)` plus a
//! handful of typed methods that return raw `serde_json::Value` or lightly
//! typed models. Sends retry transient failures (connect/timeout, 429,
//! 502/503/504) twice with full jitter under a 15s budget; caching and
//! semantic retries remain the caller's job.

pub mod intel;

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use reqwest::{header, Client, RequestBuilder};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::Mutex;
use tokio::time::sleep;

/// Per-service credential bundle.
#[derive(Debug, Clone)]
pub enum Auth {
    None,
    /// Sent as `Authorization: Bearer <key>` by default; MISP overrides this
    /// with a raw `Authorization: <key>` header per its API convention.
    ApiKey(String),
    Basic {
        user: String,
        pass: String,
    },
}

impl Auth {
    fn apply(&self, req: RequestBuilder) -> RequestBuilder {
        match self {
            Auth::None => req,
            Auth::ApiKey(k) => req.bearer_auth(k),
            Auth::Basic { user, pass } => req.basic_auth(user, Some(pass)),
        }
    }
}

fn env(key: &str) -> Option<String> {
    std::env::var(key).ok().filter(|s| !s.is_empty())
}

fn join(base: &str, path: &str) -> String {
    format!("{}{}", base.trim_end_matches('/'), path)
}

async fn json_ok<T: DeserializeOwned>(resp: reqwest::Response) -> Result<T> {
    let resp = resp.error_for_status().context("upstream HTTP error")?;
    Ok(resp.json::<T>().await.context("decode upstream JSON")?)
}

// ---------------------------------------------------------------------------
// Transient-failure retry
// ---------------------------------------------------------------------------

/// Retries after the first send (3 sends total).
const MAX_TRANSIENT_RETRIES: u32 = 2;
/// Per-call wall-clock ceiling across retries (the resilience contract).
const CALL_BUDGET: Duration = Duration::from_secs(15);
/// `sleep = random(0, min(4s, 500ms * 2^attempt))` — full jitter.
const RETRY_BACKOFF_CAP: Duration = Duration::from_secs(4);

/// Full-jitter backoff clipped to `cap` and the remaining call budget.
fn retry_sleep(attempt: u32, remaining: Duration) -> Duration {
    let cap = Duration::from_millis(500)
        .saturating_mul(1u32 << attempt.min(6))
        .min(RETRY_BACKOFF_CAP)
        .min(remaining);
    Duration::from_millis(fastrand::u64(..=cap.as_millis() as u64))
}

/// Send `req`, retrying transient failures — connect/timeout errors and
/// 429/502/503/504 — up to [`MAX_TRANSIENT_RETRIES`] times with full jitter.
/// Other statuses (4xx incl. auth failures) return untouched for the
/// caller's `error_for_status`. Total time stays under [`CALL_BUDGET`].
async fn send_with_retry(req: RequestBuilder) -> Result<reqwest::Response> {
    if req.try_clone().is_none() {
        // Body cannot be replayed (stream); a retry would send nothing.
        return req.send().await.context("upstream request failed");
    }
    let deadline = Instant::now() + CALL_BUDGET;
    let mut attempt = 0u32;
    loop {
        let attempt_req = req
            .try_clone()
            .expect("replayable body checked before the retry loop");
        match attempt_req.send().await {
            Ok(resp) => {
                let code = resp.status().as_u16();
                let transient = code == 429 || matches!(code, 502 | 503 | 504);
                if !transient || attempt >= MAX_TRANSIENT_RETRIES {
                    return Ok(resp);
                }
                let remaining = deadline.saturating_duration_since(Instant::now());
                if remaining.is_zero() {
                    return Ok(resp);
                }
                tracing::warn!(
                    status = code,
                    attempt,
                    "transient upstream status; retrying"
                );
                sleep(retry_sleep(attempt, remaining)).await;
            }
            Err(e) => {
                let transient = e.is_connect() || e.is_timeout();
                if !transient || attempt >= MAX_TRANSIENT_RETRIES {
                    return Err(e).context("upstream request failed");
                }
                let remaining = deadline.saturating_duration_since(Instant::now());
                if remaining.is_zero() {
                    return Err(e).context("upstream request failed");
                }
                tracing::warn!(error = %e, attempt, "transient transport error; retrying");
                sleep(retry_sleep(attempt, remaining)).await;
            }
        }
        attempt += 1;
    }
}

// ---------------------------------------------------------------------------
// OpenSearch
// ---------------------------------------------------------------------------

/// OpenSearch / Elasticsearch `_search` client.
#[derive(Clone)]
pub struct OpensearchClient {
    base: String,
    http: Client,
    auth: Auth,
}

impl OpensearchClient {
    pub fn new(base_url: impl Into<String>, auth: Auth) -> Self {
        Self {
            base: base_url.into().trim_end_matches('/').to_string(),
            http: Client::new(),
            auth,
        }
    }

    /// Indexes `doc` into `index` as a new document (`POST /{index}/_doc`).
    pub async fn index_doc(&self, index: &str, doc: &Value) -> Result<Value> {
        let req = self
            .auth
            .apply(self.http.post(join(&self.base, &format!("/{index}/_doc"))))
            .json(doc);
        json_ok(send_with_retry(req).await.context("opensearch index_doc")?).await
    }

    /// `POST {base}/{path}` with a JSON body (monitors, plugins, etc.).
    pub async fn post_json(&self, path: &str, body: &Value) -> Result<Value> {
        let req = self
            .auth
            .apply(self.http.post(join(&self.base, path)))
            .json(body);
        json_ok(send_with_retry(req).await.context("opensearch post")?).await
    }

    /// `POST {base}/{index}/_search` with the given query body. `q` is passed
    /// through as the optional URI `?q=` query-string search parameter.
    pub async fn query(&self, index: &str, q: &str, body: Value) -> Result<Value> {
        let req = self
            .auth
            .apply(
                self.http
                    .post(join(&self.base, &format!("/{index}/_search"))),
            )
            .query(&[("q", q)])
            .json(&body);
        json_ok(send_with_retry(req).await.context("opensearch query")?).await
    }
}

// ---------------------------------------------------------------------------
// TheHive
// ---------------------------------------------------------------------------

/// New case payload for TheHive (`POST /api/v1/case`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewCase {
    pub title: String,
    pub description: String,
    #[serde(default)]
    pub severity: u8,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub tlp: u8,
}

/// A case as returned by TheHive; unknown fields are preserved in `extra`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Case {
    #[serde(rename = "_id")]
    pub id: String,
    #[serde(default)]
    pub title: String,
    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
}

/// New observable payload (`POST /api/v1/case/{id}/observable`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewObservable {
    #[serde(rename = "dataType")]
    pub data_type: String,
    pub data: String,
    #[serde(default)]
    pub message: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub tlp: Option<u8>,
    #[serde(default)]
    pub ioc: bool,
}

/// An observable as returned by TheHive.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Observable {
    #[serde(rename = "_id")]
    pub id: String,
    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
}

/// TheHive case-management client.
#[derive(Clone)]
pub struct ThehiveClient {
    base: String,
    http: Client,
    auth: Auth,
}

impl ThehiveClient {
    pub fn new(base_url: impl Into<String>, auth: Auth) -> Self {
        Self {
            base: base_url.into().trim_end_matches('/').to_string(),
            http: Client::new(),
            auth,
        }
    }

    /// `POST /api/v1/case`.
    /// `POST /api/v1/query` — TheHive's search DSL; `query` is one filter.
    pub async fn query(&self, query: Value) -> Result<Value> {
        let req = self
            .auth
            .apply(self.http.post(join(&self.base, "/api/v1/query")))
            .json(&serde_json::json!({ "query": [query] }));
        json_ok(send_with_retry(req).await.context("thehive query")?).await
    }

    pub async fn create_case(&self, case: &NewCase) -> Result<Case> {
        let req = self
            .auth
            .apply(self.http.post(join(&self.base, "/api/v1/case")))
            .json(case);
        json_ok(send_with_retry(req).await.context("thehive create_case")?).await
    }

    /// `POST /api/v1/case/{case_id}/observable`.
    pub async fn add_observable(
        &self,
        case_id: &str,
        observable: &NewObservable,
    ) -> Result<Observable> {
        let req = self
            .auth
            .apply(self.http.post(join(
                &self.base,
                &format!("/api/v1/case/{case_id}/observable"),
            )))
            .json(observable);
        json_ok(
            send_with_retry(req)
                .await
                .context("thehive add_observable")?,
        )
        .await
    }

    /// `PATCH /api/v1/case/{case_id}` with an arbitrary JSON patch body.
    pub async fn update_case(&self, case_id: &str, patch: &Value) -> Result<Case> {
        let req = self
            .auth
            .apply(
                self.http
                    .patch(join(&self.base, &format!("/api/v1/case/{case_id}"))),
            )
            .json(patch);
        json_ok(send_with_retry(req).await.context("thehive update_case")?).await
    }
}

// ---------------------------------------------------------------------------
// Cortex
// ---------------------------------------------------------------------------

/// Cortex analyzer runner.
#[derive(Clone)]
pub struct CortexClient {
    base: String,
    http: Client,
    auth: Auth,
}

impl CortexClient {
    pub fn new(base_url: impl Into<String>, auth: Auth) -> Self {
        Self {
            base: base_url.into().trim_end_matches('/').to_string(),
            http: Client::new(),
            auth,
        }
    }

    /// `POST /api/analyzer/{analyzer_id}/run` — returns the submitted job.
    pub async fn run_analyzer(
        &self,
        analyzer_id: &str,
        data_type: &str,
        data: &str,
    ) -> Result<Value> {
        let body = serde_json::json!({
            "data": data,
            "dataType": data_type,
            "tlp": 2,
        });
        let req = self
            .auth
            .apply(self.http.post(join(
                &self.base,
                &format!("/api/analyzer/{analyzer_id}/run"),
            )))
            .json(&body);
        json_ok(send_with_retry(req).await.context("cortex run_analyzer")?).await
    }
}

// ---------------------------------------------------------------------------
// MISP
// ---------------------------------------------------------------------------

/// A MISP event from `restSearch`; unknown fields land in `extra`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    #[serde(rename = "id", deserialize_with = "de_string_or_num")]
    pub id: String,
    #[serde(default)]
    pub info: String,
    #[serde(default)]
    pub date: String,
    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
}

/// New attribute payload for `POST /attributes/add/{event_id}`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewAttribute {
    #[serde(rename = "type")]
    pub attr_type: String,
    pub category: String,
    pub value: String,
    #[serde(default)]
    pub comment: Option<String>,
    #[serde(default, rename = "to_ids")]
    pub to_ids: bool,
}

fn de_string_or_num<'de, D>(d: D) -> std::result::Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    match Value::deserialize(d)? {
        Value::String(s) => Ok(s),
        Value::Number(n) => Ok(n.to_string()),
        _ => Ok(String::new()),
    }
}

fn de_opt_string_or_num<'de, D>(d: D) -> std::result::Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Ok(match Value::deserialize(d)? {
        Value::String(s) => Some(s),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    })
}

/// Values from a `warninglists/checkValue` response that matched at least one
/// list. Shape: `{"<value>": [{"id": "10", "name": "..."}, ...]}`.
fn warninglist_matches(raw: &Value) -> Vec<String> {
    raw.as_object()
        .map(|m| {
            m.iter()
                .filter(|(_, hits)| hits.as_array().is_some_and(|a| !a.is_empty()))
                .map(|(v, _)| v.clone())
                .collect()
        })
        .unwrap_or_default()
}

/// A MISP attribute from `POST /attributes/restSearch`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MispAttribute {
    /// Attribute id (numeric on the wire).
    #[serde(rename = "id", default, deserialize_with = "de_string_or_num")]
    pub id: String,
    /// Owning event id.
    #[serde(
        rename = "event_id",
        default,
        deserialize_with = "de_opt_string_or_num"
    )]
    pub event_id: Option<String>,
    /// Attribute type (`ip-dst`, `domain`, `sha256`, ...).
    #[serde(rename = "type", default)]
    pub attr_type: String,
    /// Attribute category.
    #[serde(default)]
    pub category: String,
    /// Indicator value.
    #[serde(default)]
    pub value: String,
    /// Whether the attribute is auto-exported as an indicator.
    #[serde(default)]
    pub to_ids: bool,
    /// Unix timestamp string.
    #[serde(default)]
    pub timestamp: Option<String>,
    /// Analyst comment.
    #[serde(default)]
    pub comment: Option<String>,
    /// Attached tags.
    #[serde(rename = "Tag", default)]
    pub tags: Vec<MispTag>,
    /// Unknown fields.
    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
}

/// A MISP tag attached to an attribute or event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MispTag {
    /// Tag name, e.g. `tlp:amber` or `misp-galaxy:mitre-attack-pattern="..."`
    #[serde(default)]
    pub name: String,
}

/// MISP threat-intel client. Auth is the raw `Authorization` header (MISP's
/// convention) plus `Accept: application/json`.
#[derive(Clone)]
pub struct MispClient {
    base: String,
    http: Client,
    auth: Auth,
}

impl MispClient {
    pub fn new(base_url: impl Into<String>, auth: Auth) -> Self {
        Self {
            base: base_url.into().trim_end_matches('/').to_string(),
            http: Client::new(),
            auth,
        }
    }

    /// MISP wants the raw key in `Authorization`, not a Bearer prefix.
    fn authed(&self, req: RequestBuilder) -> RequestBuilder {
        let req = req.header(header::ACCEPT, "application/json");
        match &self.auth {
            Auth::ApiKey(k) => req.header(header::AUTHORIZATION, k),
            other => other.apply(req),
        }
    }

    /// `POST /events/restSearch` — full-text value search, returns events.
    pub async fn search(&self, value: &str) -> Result<Vec<Event>> {
        let body = serde_json::json!({
            "returnFormat": "json",
            "value": value,
            "limit": 50,
        });
        let req = self
            .authed(self.http.post(join(&self.base, "/events/restSearch")))
            .json(&body);
        let resp = send_with_retry(req).await.context("misp search")?;
        let raw: Value = json_ok(resp).await?;
        // Shape: {"response": [{"Event": {...}}, ...]}
        let mut events = Vec::new();
        if let Some(rows) = raw.get("response").and_then(|r| r.as_array()) {
            for row in rows {
                let Some(ev) = row.get("Event") else { continue };
                match serde_json::from_value::<Event>(ev.clone()) {
                    Ok(e) => events.push(e),
                    Err(e) => tracing::warn!(error = %e, "misp event row skipped"),
                }
            }
        }
        Ok(events)
    }

    /// `POST /attributes/add/{event_id}`.
    pub async fn add_attribute(&self, event_id: &str, attr: &NewAttribute) -> Result<Value> {
        let req = self
            .authed(
                self.http
                    .post(join(&self.base, &format!("/attributes/add/{event_id}"))),
            )
            .json(attr);
        json_ok(send_with_retry(req).await.context("misp add_attribute")?).await
    }

    /// `POST /attributes/restSearch` — attribute-level search; the shape is
    /// `{"response": {"Attribute": [...]}}`.
    pub async fn search_attributes(&self, value: &str, limit: u32) -> Result<Vec<MispAttribute>> {
        let body = serde_json::json!({
            "returnFormat": "json",
            "value": value,
            "limit": limit,
        });
        let req = self
            .authed(self.http.post(join(&self.base, "/attributes/restSearch")))
            .json(&body);
        let raw: Value = json_ok(
            send_with_retry(req)
                .await
                .context("misp search_attributes")?,
        )
        .await?;
        Ok(raw
            .pointer("/response/Attribute")
            .and_then(Value::as_array)
            .map(|rows| {
                rows.iter()
                    .filter_map(|r| match serde_json::from_value(r.clone()) {
                        Ok(attr) => Some(attr),
                        Err(e) => {
                            tracing::warn!(error = %e, "misp attribute row skipped");
                            None
                        }
                    })
                    .collect()
            })
            .unwrap_or_default())
    }

    /// `GET /events/view/{event_id}` — full event with attributes, objects,
    /// tags and galaxies.
    pub async fn event(&self, event_id: &str) -> Result<Value> {
        let req = self.authed(
            self.http
                .get(join(&self.base, &format!("/events/view/{event_id}"))),
        );
        json_ok(send_with_retry(req).await.context("misp event")?).await
    }

    /// `POST /warninglists/checkValue` — returns the subset of `values` that
    /// match at least one warninglist (known-benign ranges, etc.).
    pub async fn check_warninglists(&self, values: &[String]) -> Result<Vec<String>> {
        let req = self
            .authed(self.http.post(join(&self.base, "/warninglists/checkValue")))
            .json(values);
        let raw: Value = json_ok(
            send_with_retry(req)
                .await
                .context("misp check_warninglists")?,
        )
        .await?;
        Ok(warninglist_matches(&raw))
    }

    /// `POST /sightings/add` — record sightings for attribute values.
    pub async fn add_sightings(&self, values: &[String], source: Option<&str>) -> Result<Value> {
        let mut body = serde_json::json!({ "values": values });
        if let Some(src) = source {
            body["source"] = Value::String(src.to_string());
        }
        let req = self
            .authed(self.http.post(join(&self.base, "/sightings/add")))
            .json(&body);
        json_ok(send_with_retry(req).await.context("misp add_sightings")?).await
    }
}

// ---------------------------------------------------------------------------
// Wazuh
// ---------------------------------------------------------------------------

/// Wazuh manager API client. Authenticates lazily against
/// `POST /security/user/authenticate` and caches the JWT.
#[derive(Clone)]
pub struct WazuhClient {
    base: String,
    http: Client,
    auth: Auth,
    token: Arc<Mutex<Option<String>>>,
}

impl WazuhClient {
    pub fn new(base_url: impl Into<String>, auth: Auth) -> Self {
        Self {
            base: base_url.into().trim_end_matches('/').to_string(),
            http: Client::new(),
            auth,
            token: Arc::new(Mutex::new(None)),
        }
    }

    /// Fetch (once) and cache a JWT.
    async fn token(&self) -> Result<String> {
        let mut guard = self.token.lock().await;
        if let Some(t) = guard.as_ref() {
            return Ok(t.clone());
        }
        let req = self.auth.apply(
            self.http
                .post(join(&self.base, "/security/user/authenticate")),
        );
        let resp = send_with_retry(req).await.context("wazuh authenticate")?;
        let body: Value = json_ok(resp).await?;
        let token = body
            .pointer("/data/token")
            .or_else(|| body.get("token"))
            .and_then(|t| t.as_str())
            .context("wazuh auth response missing token")?
            .to_string();
        *guard = Some(token.clone());
        Ok(token)
    }

    async fn authed(&self, req_builder: impl FnOnce() -> RequestBuilder) -> Result<RequestBuilder> {
        let token = self.token().await?;
        Ok(req_builder().bearer_auth(token))
    }

    /// `GET /agents` — raw response value (`data.affected_items` inside).
    pub async fn list_agents(&self) -> Result<Value> {
        let req = self
            .authed(|| self.http.get(join(&self.base, "/agents")))
            .await?;
        json_ok(send_with_retry(req).await.context("wazuh list_agents")?).await
    }

    /// `PUT /active-response?agents_list={agent_id}` — run an AR command.
    pub async fn active_response(
        &self,
        agent_id: &str,
        command: &str,
        args: Vec<String>,
    ) -> Result<Value> {
        let body = serde_json::json!({
            "command": command,
            "arguments": args,
            "custom": true,
            "alert": {},
        });
        let req = self
            .authed(|| {
                self.http
                    .put(join(&self.base, "/active-response"))
                    .query(&[("agents_list", agent_id)])
            })
            .await?
            .json(&body);
        json_ok(
            send_with_retry(req)
                .await
                .context("wazuh active_response")?,
        )
        .await
    }
}

// ---------------------------------------------------------------------------
// Shuffle (SOAR workflows)
// ---------------------------------------------------------------------------

/// Shuffle workflow executor.
#[derive(Clone)]
pub struct ShuffleClient {
    base: String,
    http: Client,
    auth: Auth,
}

impl ShuffleClient {
    pub fn new(base_url: impl Into<String>, auth: Auth) -> Self {
        Self {
            base: base_url.into().trim_end_matches('/').to_string(),
            http: Client::new(),
            auth,
        }
    }

    /// `POST /api/v1/workflows/{workflow_id}/execute` — body passed through.
    pub async fn run_workflow(&self, workflow_id: &str, body: Value) -> Result<Value> {
        let req = self
            .auth
            .apply(self.http.post(join(
                &self.base,
                &format!("/api/v1/workflows/{workflow_id}/execute"),
            )))
            .json(&body);
        json_ok(send_with_retry(req).await.context("shuffle run_workflow")?).await
    }
}

// ---------------------------------------------------------------------------
// Env-driven config
// ---------------------------------------------------------------------------

/// Connection settings for every connector, read from the environment.
#[derive(Debug, Clone, Default)]
pub struct ConnectorConfig {
    pub opensearch_url: Option<String>,
    pub opensearch_user: Option<String>,
    pub opensearch_pass: Option<String>,
    pub wazuh_api_url: Option<String>,
    pub wazuh_api_user: Option<String>,
    pub wazuh_api_pass: Option<String>,
    pub thehive_url: Option<String>,
    pub thehive_api_key: Option<String>,
    pub cortex_url: Option<String>,
    pub cortex_api_key: Option<String>,
    pub misp_url: Option<String>,
    pub misp_api_key: Option<String>,
    pub shuffle_url: Option<String>,
    pub shuffle_api_key: Option<String>,
    pub nvd_api_key: Option<String>,
}

impl ConnectorConfig {
    /// Read `OPENSEARCH_*`, `WAZUH_API_*`, `THEHIVE_*`, `CORTEX_*`, `MISP_*`,
    /// and `SHUFFLE_*` env vars. Missing vars become `None`.
    pub fn from_env() -> Self {
        Self {
            opensearch_url: env("OPENSEARCH_URL"),
            opensearch_user: env("OPENSEARCH_USER"),
            opensearch_pass: env("OPENSEARCH_PASS"),
            wazuh_api_url: env("WAZUH_API_URL"),
            wazuh_api_user: env("WAZUH_API_USER"),
            wazuh_api_pass: env("WAZUH_API_PASS"),
            thehive_url: env("THEHIVE_URL"),
            thehive_api_key: env("THEHIVE_API_KEY"),
            cortex_url: env("CORTEX_URL"),
            cortex_api_key: env("CORTEX_API_KEY"),
            misp_url: env("MISP_URL"),
            misp_api_key: env("MISP_API_KEY"),
            shuffle_url: env("SHUFFLE_URL"),
            shuffle_api_key: env("SHUFFLE_API_KEY"),
            nvd_api_key: env("NVD_API_KEY"),
        }
    }

    /// Builds an OpenSearch client if `OPENSEARCH_URL` is set.
    pub fn build_opensearch_client(&self) -> Option<OpensearchClient> {
        let url = self.opensearch_url.clone()?;
        let auth = match (&self.opensearch_user, &self.opensearch_pass) {
            (Some(u), Some(p)) => Auth::Basic {
                user: u.clone(),
                pass: p.clone(),
            },
            _ => Auth::None,
        };
        Some(OpensearchClient::new(url, auth))
    }

    /// Wazuh client if `WAZUH_API_URL` is set.
    pub fn build_wazuh_client(&self) -> Option<WazuhClient> {
        let url = self.wazuh_api_url.clone()?;
        let auth = match (&self.wazuh_api_user, &self.wazuh_api_pass) {
            (Some(u), Some(p)) => Auth::Basic {
                user: u.clone(),
                pass: p.clone(),
            },
            _ => Auth::None,
        };
        Some(WazuhClient::new(url, auth))
    }

    /// TheHive client if `THEHIVE_URL` is set.
    pub fn build_thehive_client(&self) -> Option<ThehiveClient> {
        let url = self.thehive_url.clone()?;
        let auth = self
            .thehive_api_key
            .clone()
            .map(Auth::ApiKey)
            .unwrap_or(Auth::None);
        Some(ThehiveClient::new(url, auth))
    }

    /// Cortex client if `CORTEX_URL` is set.
    pub fn build_cortex_client(&self) -> Option<CortexClient> {
        let url = self.cortex_url.clone()?;
        let auth = self
            .cortex_api_key
            .clone()
            .map(Auth::ApiKey)
            .unwrap_or(Auth::None);
        Some(CortexClient::new(url, auth))
    }

    /// MISP client if `MISP_URL` is set.
    pub fn build_misp_client(&self) -> Option<MispClient> {
        let url = self.misp_url.clone()?;
        let auth = self
            .misp_api_key
            .clone()
            .map(Auth::ApiKey)
            .unwrap_or(Auth::None);
        Some(MispClient::new(url, auth))
    }

    /// Shuffle client if `SHUFFLE_URL` is set.
    pub fn build_shuffle_client(&self) -> Option<ShuffleClient> {
        let url = self.shuffle_url.clone()?;
        let auth = self
            .shuffle_api_key
            .clone()
            .map(Auth::ApiKey)
            .unwrap_or(Auth::None);
        Some(ShuffleClient::new(url, auth))
    }

    /// NVD client; always available, with the faster rate limit when
    /// `NVD_API_KEY` is set.
    pub fn build_nvd_client(&self) -> NvdClient {
        NvdClient::new(self.nvd_api_key.clone())
    }
}

// ---------------------------------------------------------------------------
// NVD (National Vulnerability Database) API 2.0
// ---------------------------------------------------------------------------

const NVD_BASE: &str = "https://services.nvd.nist.gov";
/// Public rate limit: ~5 req/30s => one request every 6s.
const NVD_INTERVAL_ANON: Duration = Duration::from_millis(6_000);
/// With an API key: ~50 req/30s => one request every 600ms.
const NVD_INTERVAL_KEY: Duration = Duration::from_millis(600);
const NVD_MAX_RETRIES: u32 = 3;

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
                        anyhow::bail!("NVD HTTP {status}");
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
            Err(e) if e.to_string().contains("404") => return Ok(None),
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

#[cfg(test)]
mod tests {
    use super::*;

    /// `attributes/restSearch` items carry numeric ids and nested `Tag` rows.
    #[test]
    fn misp_attribute_parses_rest_search_item() {
        let raw = serde_json::json!({
            "id": "12345",
            "event_id": "7",
            "type": "ip-dst",
            "category": "Network activity",
            "value": "203.0.113.9",
            "to_ids": true,
            "timestamp": "1712000000",
            "comment": "C2",
            "Tag": [{"name": "tlp:amber"}, {"name": "misp-galaxy:mitre-attack-pattern=\"C2\""}],
            "Event": {"info": "ignored by flatten"}
        });
        let a: MispAttribute = serde_json::from_value(raw).unwrap();
        assert_eq!(a.id, "12345");
        assert_eq!(a.event_id.as_deref(), Some("7"));
        assert!(a.to_ids);
        assert_eq!(a.tags.len(), 2);
        assert_eq!(a.tags[0].name, "tlp:amber");
        assert!(a.extra.contains_key("Event"));
    }

    /// Missing event_id stays `None` instead of failing the row.
    #[test]
    fn misp_attribute_tolerates_missing_event_id() {
        let a: MispAttribute = serde_json::from_value(serde_json::json!({
            "id": 1, "type": "domain", "value": "example.org"
        }))
        .unwrap();
        assert_eq!(a.event_id, None);
        assert!(!a.to_ids);
    }

    /// `checkValue` maps values to hit lists; empty lists are not matches.
    #[test]
    fn warninglist_matches_skips_empty_hit_lists() {
        let raw = serde_json::json!({
            "10.128.0.2": [{"id": "10", "name": "Wikimedia ranges"}],
            "8.8.8.8": []
        });
        assert_eq!(warninglist_matches(&raw), vec!["10.128.0.2".to_string()]);
        assert!(warninglist_matches(&serde_json::json!([])).is_empty());
    }
}

#[cfg(test)]
mod retry_tests {
    use super::*;

    #[test]
    fn retry_sleep_stays_within_budget() {
        for attempt in 0..6 {
            for _ in 0..20 {
                let sleep = retry_sleep(attempt, Duration::from_secs(15));
                assert!(sleep <= RETRY_BACKOFF_CAP, "{sleep:?} exceeds the cap");
                assert!(sleep <= Duration::from_secs(15));
            }
        }
        // A tiny remaining budget clips the sleep to that budget.
        for _ in 0..20 {
            assert!(retry_sleep(5, Duration::from_millis(10)) <= Duration::from_millis(10));
        }
    }

    #[test]
    fn b64url_is_padding_free() {
        assert_eq!(intel::b64url(b"https://a.b/?=>"), "aHR0cHM6Ly9hLmIvPz0-");
    }
}
