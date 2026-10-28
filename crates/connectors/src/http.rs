//! Shared credential bundle and transport helpers.
//!
//! Every client sends through [`send_with_retry`]: connect/timeout, 429 and
//! 502/503/504 are retried twice with full jitter inside a 15s budget.
use std::time::{Duration, Instant};
use tokio::time::sleep;

use anyhow::{Context, Result};

use reqwest::RequestBuilder;

use serde::de::DeserializeOwned;

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
    pub(crate) fn apply(&self, req: RequestBuilder) -> RequestBuilder {
        match self {
            Auth::None => req,
            Auth::ApiKey(k) => req.bearer_auth(k),
            Auth::Basic { user, pass } => req.basic_auth(user, Some(pass)),
        }
    }
}

pub(crate) fn env(key: &str) -> Option<String> {
    std::env::var(key).ok().filter(|s| !s.is_empty())
}

pub(crate) fn join(base: &str, path: &str) -> String {
    format!("{}{}", base.trim_end_matches('/'), path)
}

/// Validates an untrusted identifier before it is interpolated into an
/// upstream URL path. Accepts `[A-Za-z0-9._~-]{1,128}` only — `/`, `?`, `#`,
/// `&`, `=` and `..`-traversal via separators are all rejected, so a tool
/// arg can never escape the intended endpoint or smuggle query params.
pub fn segment<'a>(id: &'a str, what: &str) -> Result<&'a str> {
    let ok = !id.is_empty()
        && id.len() <= 128
        && !id.starts_with('.')
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '~' | '-'));
    anyhow::ensure!(ok, "invalid {what}: `{id}`");
    Ok(id)
}

/// `reqwest::Error` Display embeds the full request URL — query string and
/// all — which would leak credentials such as Shodan's `?key=` into logs
/// and tool results. Always surface errors through this.
pub(crate) fn without_url(e: reqwest::Error) -> reqwest::Error {
    e.without_url()
}

pub(crate) async fn json_ok<T: DeserializeOwned>(resp: reqwest::Response) -> Result<T> {
    let resp = resp
        .error_for_status()
        .map_err(without_url)
        .context("upstream HTTP error")?;
    resp.json::<T>().await.context("decode upstream JSON")
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
pub(crate) fn retry_sleep(attempt: u32, remaining: Duration) -> Duration {
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
pub(crate) async fn send_with_retry(req: RequestBuilder) -> Result<reqwest::Response> {
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
                let transient = code == 429 || matches!(code, 502..=504);
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
                    return Err(without_url(e)).context("upstream request failed");
                }
                let remaining = deadline.saturating_duration_since(Instant::now());
                if remaining.is_zero() {
                    return Err(without_url(e)).context("upstream request failed");
                }
                tracing::warn!(error = %e.without_url(), attempt, "transient transport error; retrying");
                sleep(retry_sleep(attempt, remaining)).await;
            }
        }
        attempt += 1;
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
        assert_eq!(
            crate::intel::b64url(b"https://a.b/?=>"),
            "aHR0cHM6Ly9hLmIvPz0-"
        );
    }
}
