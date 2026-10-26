//! Wazuh manager API client (agents, active response).
use std::sync::Arc;
use tokio::sync::Mutex;

use anyhow::{Context, Result};

use reqwest::{Client, RequestBuilder};

use serde_json::Value;

use crate::http::{join, json_ok, send_with_retry, Auth};

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
