//! Tool invocation gateway: how agents reach the tool server.

use std::time::Duration;

use agent_protocol::tool::{Decision, DecisionKind, ToolCall, ToolResult};
use tracing::warn;

use crate::error::AgentError;

/// Retries after the first attempt (4 sends total).
const MAX_RETRIES: u32 = 3;
/// `sleep = random(0, min(8s, 500ms * 2^attempt))` — full jitter.
const BASE_BACKOFF_MS: u64 = 500;
const BACKOFF_CAP_MS: u64 = 8_000;
/// Per-request timeout, matching the resilience contract in
/// `docs/services.md`.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);

/// Full-jitter backoff: uniform in `0 ..= min(cap, 500ms * 2^attempt)`.
fn backoff(attempt: u32) -> Duration {
    let cap = BASE_BACKOFF_MS
        .saturating_mul(1u64 << attempt.min(6))
        .min(BACKOFF_CAP_MS);
    Duration::from_millis(fastrand::u64(..=cap))
}

/// tool-server `/invoke` wire shape: the policy verdict plus the result.
#[derive(serde::Deserialize)]
struct InvokeResponse {
    decision: Decision,
    result: ToolResult,
}

/// Dispatch channel through which agents invoke [`ToolCall`]s.
#[async_trait::async_trait]
pub trait ToolGateway: Send + Sync {
    /// Invoke `call` and return its result.
    async fn invoke(&self, call: &ToolCall) -> Result<ToolResult, AgentError>;
}

/// [`ToolGateway`] that POSTs the [`ToolCall`] as JSON to `{base}/invoke`.
///
/// Calls are idempotent-keyed, so transient failures retry: connect/timeout
/// errors and results the server flagged `retryable` get up to
/// [`MAX_RETRIES`] retries with full-jitter backoff. Deny verdicts and
/// idempotency-mismatch results are never retried.
pub struct HttpToolGateway {
    client: reqwest::Client,
    base_url: String,
}

impl HttpToolGateway {
    /// Create a gateway targeting `base_url` (e.g. `http://tool-server:8080`).
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            client: reqwest::Client::builder()
                .timeout(REQUEST_TIMEOUT)
                .build()
                .expect("reqwest client builder"),
            base_url: base_url.into(),
        }
    }

    /// One `/invoke` round trip. The returned flag is the server's retry
    /// hint: `true` only on an allow verdict whose result carries the
    /// `retryable` marker (idempotency claim in flight, store unavailable).
    async fn try_invoke(&self, url: &str, call: &ToolCall) -> Result<(ToolResult, bool), AgentError> {
        let resp = self
            .client
            .post(url)
            .json(call)
            .send()
            .await
            .map_err(AgentError::Http)?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Ok((
                ToolResult {
                    call_id: call.id,
                    ok: false,
                    output: serde_json::Value::Null,
                    error: Some(format!("gateway returned {status}: {body}")),
                },
                false,
            ));
        }

        let parsed = resp.json::<InvokeResponse>().await.map_err(AgentError::Http)?;
        // Only allow verdicts may carry a retry hint; deny/approval verdicts
        // and mismatch errors are terminal by definition.
        let retryable =
            parsed.decision.kind == DecisionKind::Allow && parsed.result.is_retryable();
        Ok((parsed.result, retryable))
    }
}

#[async_trait::async_trait]
impl ToolGateway for HttpToolGateway {
    async fn invoke(&self, call: &ToolCall) -> Result<ToolResult, AgentError> {
        let url = format!("{}/invoke", self.base_url.trim_end_matches('/'));
        let mut attempt = 0u32;
        loop {
            let retry = match self.try_invoke(&url, call).await {
                Ok((result, retryable)) => {
                    if !retryable || attempt >= MAX_RETRIES {
                        return Ok(result);
                    }
                    format!("server flagged result retryable: {:?}", result.error)
                }
                Err(e) => {
                    let transient = matches!(
                        &e,
                        AgentError::Http(err) if err.is_connect() || err.is_timeout()
                    );
                    if !transient || attempt >= MAX_RETRIES {
                        return Err(e);
                    }
                    format!("transport error: {e}")
                }
            };
            warn!(
                tool = %call.tool,
                attempt,
                max_retries = MAX_RETRIES,
                reason = %retry,
                "tool invoke failed transiently; retrying"
            );
            tokio::time::sleep(backoff(attempt)).await;
            attempt += 1;
        }
    }
}
