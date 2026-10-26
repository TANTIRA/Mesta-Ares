//! LLM provider abstraction for ARES agents.
//!
//! Defines the [`LlmProvider`] trait, the request/response types agents use,
//! an [`AnthropicProvider`] talking to the Anthropic Messages API (including
//! Ollama's compatible endpoint), a [`SystemOneClient`] for Ollama's local
//! decision models, and a [`MockProvider`] for tests.

#![warn(missing_docs)]

mod anthropic;
mod error;
mod systemone;

pub use anthropic::AnthropicProvider;
pub use error::LlmError;
pub use systemone::{SystemOneClient, DEFAULT_BASE_URL as DEFAULT_OLLAMA_URL};

use serde::{Deserialize, Serialize};

/// Default model for routine steps (`FAST_MODEL` overrides it).
pub const FAST_MODEL: &str = "claude-haiku-4-5";
/// Default model for hard reasoning steps (`STRONG_MODEL` overrides it).
pub const STRONG_MODEL: &str = "claude-sonnet-4-6";

/// Fast-tier model, `FAST_MODEL` env overriding the [`FAST_MODEL`] default.
/// Point it at an Ollama tag (e.g. `qwen3-coder`) when `ANTHROPIC_BASE_URL`
/// targets an Ollama server.
pub fn fast_model() -> String {
    std::env::var("FAST_MODEL").unwrap_or_else(|_| FAST_MODEL.to_string())
}

/// Strong-tier model, `STRONG_MODEL` env overriding the [`STRONG_MODEL`]
/// default.
pub fn strong_model() -> String {
    std::env::var("STRONG_MODEL").unwrap_or_else(|_| STRONG_MODEL.to_string())
}

/// Role of a chat message author.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    /// System prompt context.
    System,
    /// End-user or agent input.
    User,
    /// Model output.
    Assistant,
}

/// A single chat message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    /// Author role of the message.
    pub role: Role,
    /// Text content of the message.
    pub content: String,
}

/// A provider-agnostic completion request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionRequest {
    /// Model identifier to complete with (see [`FAST_MODEL`], [`STRONG_MODEL`]).
    pub model: String,
    /// Optional system prompt applied to the request.
    pub system: Option<String>,
    /// Conversation messages in chronological order.
    pub messages: Vec<Message>,
    /// Maximum tokens the model may generate.
    pub max_tokens: u32,
    /// Optional sampling temperature.
    pub temperature: Option<f32>,
}

/// A provider-agnostic completion response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionResponse {
    /// The generated text.
    pub text: String,
    /// Tokens consumed by the prompt.
    pub input_tokens: u64,
    /// Tokens generated in the completion.
    pub output_tokens: u64,
    /// Model that produced the completion.
    pub model: String,
    /// Why generation stopped, as reported by the provider.
    pub stop_reason: Option<String>,
}

/// An LLM backend able to serve [`CompletionRequest`]s.
#[async_trait::async_trait]
pub trait LlmProvider: Send + Sync {
    /// Complete a request, returning the generated text and usage counters.
    async fn complete(&self, req: &CompletionRequest) -> Result<CompletionResponse, LlmError>;

    /// Static name identifying the provider (for logs and metrics).
    fn name(&self) -> &'static str;
}

/// A [`LlmProvider`] that always replies with a fixed string; for tests.
pub struct MockProvider {
    /// The text every completion returns.
    pub reply: String,
}

impl MockProvider {
    /// Create a mock that replies with `reply`.
    pub fn new(reply: impl Into<String>) -> Self {
        Self {
            reply: reply.into(),
        }
    }
}

#[async_trait::async_trait]
impl LlmProvider for MockProvider {
    async fn complete(&self, req: &CompletionRequest) -> Result<CompletionResponse, LlmError> {
        Ok(CompletionResponse {
            text: self.reply.clone(),
            input_tokens: 0,
            output_tokens: 0,
            model: req.model.clone(),
            stop_reason: Some("end_turn".to_string()),
        })
    }

    fn name(&self) -> &'static str {
        "mock"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn mock_provider_returns_fixed_reply() {
        let provider = MockProvider::new("pong");
        let req = CompletionRequest {
            model: FAST_MODEL.to_string(),
            system: None,
            messages: vec![Message {
                role: Role::User,
                content: "ping".to_string(),
            }],
            max_tokens: 16,
            temperature: Some(0.0),
        };
        let resp = provider.complete(&req).await.unwrap();
        assert_eq!(resp.text, "pong");
        assert_eq!(resp.model, FAST_MODEL);
        assert_eq!(provider.name(), "mock");
    }
}
