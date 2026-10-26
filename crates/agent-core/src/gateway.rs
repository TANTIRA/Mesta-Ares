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
#[derive(Debug, Clone, serde::Deserialize)]
pub struct InvokeResponse {
    /// Policy verdict for the call.
    pub decision: Decision,
    /// Execution result (a dry-run success when the verdict is not Allow).
    pub result: ToolResult,
    /// Approval id when the verdict is `ApprovalRequired`.
    #[serde(default)]
    pub approval_id: Option<String>,
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
    async fn try_invoke(
        &self,
        url: &str,
        call: &ToolCall,
    ) -> Result<(InvokeResponse, bool), AgentError> {
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
                InvokeResponse {
                    decision: Decision {
                        kind: DecisionKind::Deny,
                        reason: format!("gateway returned {status}: {body}"),
                        decided_at: chrono::Utc::now(),
                    },
                    result: ToolResult {
                        call_id: call.id,
                        ok: false,
                        output: serde_json::Value::Null,
                        error: Some(format!("gateway returned {status}: {body}")),
                        dry_run: false,
                    },
                    approval_id: None,
                },
                false,
            ));
        }

        let parsed = resp.json::<InvokeResponse>().await.map_err(AgentError::Http)?;
        // Only allow verdicts may carry a retry hint; deny/approval verdicts
        // and mismatch errors are terminal by definition.
        let retryable =
            parsed.decision.kind == DecisionKind::Allow && parsed.result.is_retryable();
        Ok((parsed, retryable))
    }
}

impl HttpToolGateway {
    /// Invokes `call` and returns its output, erroring on a non-allow verdict
    /// or a failed result. Use this for side effects that must not be skipped
    /// silently.
    pub async fn invoke_allowed(&self, call: &ToolCall) -> Result<serde_json::Value, AgentError> {
        let response = self.invoke_verdict(call).await?;
        if response.decision.kind != DecisionKind::Allow {
            return Err(AgentError::Denied(response.decision.reason));
        }
        if !response.result.ok {
            return Err(AgentError::Tool(
                response
                    .result
                    .error
                    .unwrap_or_else(|| "tool reported failure".to_string()),
            ));
        }
        Ok(response.result.output)
    }

    /// Invokes `call` and returns the full verdict plus result.
    pub async fn invoke_verdict(&self, call: &ToolCall) -> Result<InvokeResponse, AgentError> {
        let url = format!("{}/invoke", self.base_url.trim_end_matches('/'));
        let mut attempt = 0u32;
        loop {
            let retry = match self.try_invoke(&url, call).await {
                Ok((response, retryable)) => {
                    if !retryable || attempt >= MAX_RETRIES {
                        return Ok(response);
                    }
                    format!("server flagged result retryable: {:?}", response.result.error)
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

#[async_trait::async_trait]
impl ToolGateway for HttpToolGateway {
    async fn invoke(&self, call: &ToolCall) -> Result<ToolResult, AgentError> {
        Ok(self.invoke_verdict(call).await?.result)
    }
}
