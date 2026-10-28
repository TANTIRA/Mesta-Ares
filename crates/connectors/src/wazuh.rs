//! Wazuh manager API client (agents, active response).
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

use anyhow::{Context, Result};

use reqwest::{Client, RequestBuilder};

use serde_json::Value;

use crate::http::{join, json_ok, send_with_retry, Auth};

// ---------------------------------------------------------------------------
// Wazuh
// ---------------------------------------------------------------------------

/// Cached JWT plus when it was fetched.
struct CachedToken {
    value: String,
    fetched: Instant,
}

/// Token lifetime floor: Wazuh issues JWTs at `auth_token_exp_timeout`
/// (default 900s); refresh well inside that. `WAZUH_TOKEN_TTL_SECS` overrides.
fn token_ttl() -> Duration {
    Duration::from_secs(
        crate::http::env("WAZUH_TOKEN_TTL_SECS")
            .and_then(|v| v.parse().ok())
            .unwrap_or(700),
    )
}

/// Wazuh manager API client. Authenticates lazily against
/// `POST /security/user/authenticate`; the cached JWT is refreshed when it
/// ages past the TTL or when a request comes back 401.
#[derive(Clone)]
pub struct WazuhClient {
    base: String,
    http: Client,
    auth: Auth,
    token: Arc<Mutex<Option<CachedToken>>>,
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

    /// Fetch a token when none is cached or the cached one is stale.
    async fn token(&self) -> Result<String> {
        let mut guard = self.token.lock().await;
        if let Some(t) = guard.as_ref() {
            if t.fetched.elapsed() < token_ttl() {
                return Ok(t.value.clone());
            }
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
        *guard = Some(CachedToken {
            value: token.clone(),
            fetched: Instant::now(),
        });
        Ok(token)
    }

    /// Drop the cached token so the next call re-authenticates.
    async fn invalidate(&self) {
        *self.token.lock().await = None;
    }

    /// Build `req` with a fresh bearer; on a 401 re-authenticates and retries
    /// exactly once.
    async fn request(&self, req_builder: impl Fn() -> RequestBuilder) -> Result<reqwest::Response> {
        for attempt in 0..2 {
            let token = self.token().await?;
            let resp = send_with_retry(req_builder().bearer_auth(token))
                .await
                .context("wazuh request")?;
            if resp.status() != reqwest::StatusCode::UNAUTHORIZED || attempt == 1 {
                return Ok(resp);
            }
            self.invalidate().await;
        }
        unreachable!()
    }

    /// `GET /agents` — raw response value (`data.affected_items` inside).
    pub async fn list_agents(&self) -> Result<Value> {
        let resp = self
            .request(|| self.http.get(join(&self.base, "/agents")))
            .await
            .context("wazuh list_agents")?;
        json_ok(resp).await
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
        let resp = self
            .request(|| {
                self.http
                    .put(join(&self.base, "/active-response"))
                    .query(&[("agents_list", agent_id)])
                    .json(&body)
            })
            .await
            .context("wazuh active_response")?;
        json_ok(resp).await
    }
}
