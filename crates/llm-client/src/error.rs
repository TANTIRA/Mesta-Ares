//! Error type for LLM provider calls.

/// Errors an [`crate::LlmProvider`] can return.
#[derive(Debug, thiserror::Error)]
pub enum LlmError {
    /// Transport-level HTTP failure.
    #[error("http error: {0}")]
    Http(#[from] reqwest::Error),

    /// The provider answered with a non-2xx status.
    #[error("api error {status}: {body}")]
    Api {
        /// HTTP status code returned by the provider.
        status: u16,
        /// Response body returned by the provider.
        body: String,
    },

    /// The provider response contained no text content block.
    #[error("response contained no text content")]
    NoText,
}
