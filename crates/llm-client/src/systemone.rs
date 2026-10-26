//! Ollama System One decision-model client (`POST /v1/systemone`).
//!
//! Decision models (Nimble, Tev1, Clef) answer typed questions about a state
//! document instead of generating prose: `choice` picks one of 2–26 labeled
//! options, `score` returns the probability-weighted level on an ordered
//! scale. Local requests need no API key. Only `choice` and `score` are
//! wired here — they cover triage's verdict and severity.

use std::collections::HashMap;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::LlmError;

/// Default local Ollama base URL.
pub const DEFAULT_BASE_URL: &str = "http://localhost:11434";

/// A cold decision model can take minutes to load on CPU; allow for it.
const TIMEOUT: Duration = Duration::from_secs(180);

/// Client for Ollama's System One decision API.
pub struct SystemOneClient {
    http: reqwest::Client,
    base_url: String,
}

impl SystemOneClient {
    /// Client for an explicit base URL.
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            http: reqwest::Client::builder()
                .timeout(TIMEOUT)
                .build()
                .unwrap_or_else(|_| reqwest::Client::new()),
            base_url: base_url.into().trim_end_matches('/').to_string(),
        }
    }

    /// Client from `OLLAMA_BASE_URL`, falling back to [`DEFAULT_BASE_URL`].
    pub fn from_env() -> Self {
        match std::env::var("OLLAMA_BASE_URL") {
            Ok(url) if !url.is_empty() => Self::new(url),
            _ => Self::new(DEFAULT_BASE_URL),
        }
    }

    /// Raw call: ask any question set about `state`.
    pub async fn ask(
        &self,
        model: &str,
        state: &Value,
        questions: &Value,
    ) -> Result<Answers, LlmError> {
        #[derive(Serialize)]
        struct Req<'a> {
            model: &'a str,
            state: &'a Value,
            questions: &'a Value,
        }
        let resp = self
            .http
            .post(format!("{}/v1/systemone", self.base_url))
            .json(&Req {
                model,
                state,
                questions,
            })
            .send()
            .await?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(LlmError::Api {
                status: status.as_u16(),
                body,
            });
        }
        Ok(resp.json().await?)
    }

    /// Pick one label from `criteria` (`label` → description).
    pub async fn choice(
        &self,
        model: &str,
        state: &Value,
        instructions: &str,
        criteria: &[(&str, &str)],
    ) -> Result<Choice, LlmError> {
        let answers = self
            .ask(model, state, &choice_questions(instructions, criteria))
            .await?;
        answers
            .answers
            .get("answer")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .ok_or(LlmError::NoText)
    }

    /// Score `state` against ordered `criteria` (lowest level first); returns
    /// the probability-weighted level on the zero-based scale.
    pub async fn score(
        &self,
        model: &str,
        state: &Value,
        instructions: &str,
        criteria: &[&str],
    ) -> Result<f32, LlmError> {
        let answers = self
            .ask(model, state, &score_questions(instructions, criteria))
            .await?;
        answers
            .answers
            .get("score")
            .and_then(|v| v.get("score"))
            .and_then(Value::as_f64)
            .map(|s| s as f32)
            .ok_or(LlmError::NoText)
    }
}

/// Question set for a [`SystemOneClient::choice`] call.
fn choice_questions(instructions: &str, criteria: &[(&str, &str)]) -> Value {
    let criteria: serde_json::Map<String, Value> = criteria
        .iter()
        .map(|(label, desc)| (label.to_string(), json!(desc)))
        .collect();
    json!({
        "answer": {
            "type": "choice",
            "instructions": instructions,
            "criteria": criteria,
        }
    })
}

/// Question set for a [`SystemOneClient::score`] call.
fn score_questions(instructions: &str, criteria: &[&str]) -> Value {
    json!({
        "score": {
            "type": "score",
            "instructions": instructions,
            "criteria": criteria,
        }
    })
}

/// Parsed `POST /v1/systemone` response.
#[derive(Debug, Clone, Deserialize)]
pub struct Answers {
    /// Model that answered.
    #[serde(default)]
    pub model: String,
    /// Answers keyed by question name.
    #[serde(default)]
    pub answers: HashMap<String, Value>,
    /// Token usage, when reported.
    #[serde(default)]
    pub usage: Option<Value>,
}

/// A `choice` answer: the winning label and the full distribution.
#[derive(Debug, Clone, Deserialize)]
pub struct Choice {
    /// Most likely label.
    pub choice: String,
    /// Probability per label.
    #[serde(default)]
    pub probabilities: HashMap<String, f32>,
    /// How strongly the model favors one label (0–1).
    #[serde(default)]
    pub confidence: f32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn choice_questions_match_the_systemone_shape() {
        let q = choice_questions(
            "Which label fits this ticket?",
            &[("billing", "Payments"), ("bug", "Software errors")],
        );
        assert_eq!(q["answer"]["type"], "choice");
        assert_eq!(q["answer"]["criteria"]["bug"], "Software errors");
    }

    #[test]
    fn score_questions_match_the_systemone_shape() {
        let q = score_questions("How urgent?", &["Routine", "Soon", "Immediate"]);
        assert_eq!(q["score"]["type"], "score");
        assert_eq!(q["score"]["criteria"][2], "Immediate");
    }

    /// Documented `choice` response parses into a [`Choice`].
    #[test]
    fn choice_answer_parses() {
        let raw = serde_json::json!({
            "model": "nimble",
            "answers": {
                "answer": {
                    "type": "choice",
                    "choice": "bug",
                    "probabilities": {"billing": 0.0125, "bug": 0.9781, "account": 0.0093},
                    "confidence": 0.8906
                }
            },
            "usage": {"input_tokens": 174, "output_tokens": 1}
        });
        let answers: Answers = serde_json::from_value(raw).unwrap();
        let choice: Choice = serde_json::from_value(answers.answers["answer"].clone()).unwrap();
        assert_eq!(choice.choice, "bug");
        assert!((choice.probabilities["bug"] - 0.9781).abs() < 1e-6);
        assert!((choice.confidence - 0.8906).abs() < 1e-6);
    }

    /// Score answers expose the weighted level under `score`.
    #[test]
    fn score_answer_parses() {
        let answers: Answers = serde_json::from_value(serde_json::json!({
            "model": "nimble",
            "answers": {"score": {"type": "score", "score": 0.8308}}
        }))
        .unwrap();
        let s = answers.answers["score"]["score"].as_f64().unwrap();
        assert!((s - 0.8308).abs() < 1e-6);
    }
}
