//! Anthropic Messages API provider.

use serde::{Deserialize, Serialize};

use crate::{CompletionRequest, CompletionResponse, LlmError, LlmProvider, Message, Role};

/// Default base URL of the Anthropic API.
pub const DEFAULT_BASE_URL: &str = "https://api.anthropic.com";

const ANTHROPIC_VERSION: &str = "2023-06-01";

/// [`LlmProvider`] implementation backed by the Anthropic Messages API.
pub struct AnthropicProvider {
    client: reqwest::Client,
    api_key: String,
    base_url: String,
}

impl AnthropicProvider {
    /// Create a provider from an explicit API key, using the default base URL.
    pub fn new(api_key: String) -> Self {
        Self {
            client: reqwest::Client::new(),
            api_key,
            base_url: DEFAULT_BASE_URL.to_string(),
        }
    }

    /// Create a provider from the `ANTHROPIC_API_KEY` environment variable.
    /// `ANTHROPIC_BASE_URL` overrides the endpoint — set it to
    /// `http://localhost:11434` (local) or `https://ollama.com` (cloud) to
    /// run agents against Ollama's Anthropic-compatible `/v1/messages`.
    pub fn from_env() -> Result<Self, std::env::VarError> {
        let key = std::env::var("ANTHROPIC_API_KEY")?;
        let provider = match std::env::var("ANTHROPIC_BASE_URL") {
            Ok(url) if !url.is_empty() => Self::new(key).with_base_url(url),
            _ => Self::new(key),
        };
        Ok(provider)
    }

    /// Override the API base URL (for proxies and tests).
    pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = base_url.into();
        self
    }
}

#[derive(Serialize)]
struct ApiRequest<'a> {
    model: &'a str,
    max_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<&'a str>,
    messages: Vec<ApiMessage<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
}

#[derive(Serialize)]
struct ApiMessage<'a> {
    role: &'a str,
    content: &'a str,
}

#[derive(Deserialize)]
struct ApiResponse {
    content: Vec<ApiContent>,
    usage: ApiUsage,
    model: String,
    stop_reason: Option<String>,
}

#[derive(Deserialize)]
struct ApiContent {
    #[serde(rename = "type")]
    kind: String,
    text: Option<String>,
}

#[derive(Deserialize)]
struct ApiUsage {
    input_tokens: u64,
    output_tokens: u64,
}

fn wire_role(role: Role) -> &'static str {
    match role {
        Role::System | Role::User => "user",
        Role::Assistant => "assistant",
    }
}

#[async_trait::async_trait]
impl LlmProvider for AnthropicProvider {
    async fn complete(&self, req: &CompletionRequest) -> Result<CompletionResponse, LlmError> {
        let url = format!("{}/v1/messages", self.base_url.trim_end_matches('/'));
        let body = ApiRequest {
            model: &req.model,
            max_tokens: req.max_tokens,
            system: req.system.as_deref(),
            messages: req
                .messages
                .iter()
                .map(|m: &Message| ApiMessage {
                    role: wire_role(m.role),
                    content: &m.content,
                })
                .collect(),
            temperature: req.temperature,
        };

        let resp = self
            .client
            .post(&url)
            .header("x-api-key", &self.api_key)
            // ollama.com requires Bearer; Anthropic accepts either scheme.
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("anthropic-version", ANTHROPIC_VERSION)
            .json(&body)
            .send()
            .await
            .map_err(LlmError::Http)?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            tracing::debug!(status = %status, "anthropic api error");
            return Err(LlmError::Api {
                status: status.as_u16(),
                body,
            });
        }

        let api: ApiResponse = resp.json().await.map_err(LlmError::Http)?;
        let text = api
            .content
            .iter()
            .find(|c| c.kind == "text")
            .and_then(|c| c.text.clone())
            .ok_or(LlmError::NoText)?;

        Ok(CompletionResponse {
            text,
            input_tokens: api.usage.input_tokens,
            output_tokens: api.usage.output_tokens,
            model: api.model,
            stop_reason: api.stop_reason,
        })
    }

    fn name(&self) -> &'static str {
        "anthropic"
    }
}
