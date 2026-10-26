//! Bearer/API-key authentication for the Ares edge services.
//!
//! Keys load from `ARES_API_KEYS` (`key:actor,key2:actor2`) or the simpler
//! `ARES_API_KEY` (one key, actor `operator`). When no key is configured the
//! middleware runs in dev mode: requests pass and a warning is logged once, so
//! a local `docker compose up` still works.

use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

use axum::{
    extract::{Request, State},
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;
use tracing::warn;

/// Authenticated caller identity, inserted into request extensions.
#[derive(Debug, Clone)]
pub struct Actor(pub String);

/// Configured API keys mapped to the actor they authenticate as.
#[derive(Clone, Default)]
pub struct ApiKeys {
    keys: Arc<HashMap<String, String>>,
}

impl ApiKeys {
    /// Reads `ARES_API_KEYS` (`key:actor,...`) then `ARES_API_KEY`.
    pub fn from_env() -> Self {
        let mut keys = HashMap::new();
        if let Ok(spec) = std::env::var("ARES_API_KEYS") {
            for entry in spec.split(',').map(str::trim).filter(|e| !e.is_empty()) {
                let (key, actor) = entry
                    .split_once(':')
                    .map(|(k, a)| (k.trim(), a.trim()))
                    .unwrap_or((entry, "operator"));
                keys.insert(key.to_string(), actor.to_string());
            }
        }
        if let Ok(key) = std::env::var("ARES_API_KEY") {
            if !key.trim().is_empty() {
                keys.insert(key.trim().to_string(), "operator".to_string());
            }
        }
        Self {
            keys: Arc::new(keys),
        }
    }

    /// Builds a key set from explicit `(key, actor)` pairs.
    pub fn new(keys: impl IntoIterator<Item = (String, String)>) -> Self {
        Self {
            keys: Arc::new(keys.into_iter().collect()),
        }
    }

    /// `true` when at least one key is configured.
    pub fn is_enabled(&self) -> bool {
        !self.keys.is_empty()
    }

    /// Actor for `token`, when the token is valid.
    pub fn actor_for(&self, token: &str) -> Option<&str> {
        self.keys.get(token).map(String::as_str)
    }
}

/// Extracts the presented token from `Authorization: Bearer` or `X-API-Key`.
fn presented_token(req: &Request) -> Option<String> {
    if let Some(value) = req.headers().get("x-api-key") {
        if let Ok(token) = value.to_str() {
            return Some(token.trim().to_string());
        }
    }
    req.headers()
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer ").or_else(|| v.strip_prefix("bearer ")))
        .map(|t| t.trim().to_string())
}

/// Rejects unauthenticated requests when keys are configured, and records the
/// caller's [`Actor`] for downstream handlers.
pub async fn require_auth(
    State(keys): State<ApiKeys>,
    mut req: Request,
    next: Next,
) -> Response {
    if !keys.is_enabled() {
        static WARNED: OnceLock<()> = OnceLock::new();
        WARNED.get_or_init(|| {
            warn!("ARES_API_KEYS unset: running without inbound authentication");
        });
        req.extensions_mut().insert(Actor("anonymous".to_string()));
        return next.run(req).await;
    }
    let Some(token) = presented_token(&req) else {
        return unauthorized("missing credentials");
    };
    let Some(actor) = keys.actor_for(&token) else {
        return unauthorized("invalid credentials");
    };
    req.extensions_mut()
        .insert(Actor(actor.to_string()));
    next.run(req).await
}

fn unauthorized(reason: &str) -> Response {
    (
        StatusCode::UNAUTHORIZED,
        Json(json!({"error": "unauthorized", "reason": reason})),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_spec_parses_actor_pairs() {
        let keys = ApiKeys::new([
            ("k1".to_string(), "alice".to_string()),
            ("k2".to_string(), "bob".to_string()),
        ]);
        assert!(keys.is_enabled());
        assert_eq!(keys.actor_for("k1"), Some("alice"));
        assert_eq!(keys.actor_for("nope"), None);
    }

    #[test]
    fn empty_key_set_is_disabled() {
        assert!(!ApiKeys::default().is_enabled());
    }
}
