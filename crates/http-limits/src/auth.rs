//! Bearer/API-key authentication for the Ares edge services.
//!
//! Keys load from `ARES_API_KEYS` (`key:actor[:role],key2:actor2[:role2]`) or
//! the simpler `ARES_API_KEY` (one key, actor `operator`, role `operator`).
//! Roles are a comma-free token; today `approver` is the only role with
//! behavior attached (tool-server approval endpoints).
//!
//! When no key is configured the middleware runs in dev mode: requests pass
//! and a warning is logged once, so a local `docker compose up` still works.
//! `ARES_REQUIRE_AUTH=1` disables dev mode — [`ApiKeys::enforced`] then exits
//! the process when no key (or a weak key) is configured.

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

/// Minimum acceptable API-key length. Anything shorter is a password-grade
/// brute-force target; enforced when `ARES_REQUIRE_AUTH=1`.
const MIN_KEY_LEN: usize = 32;

/// Authenticated caller identity, inserted into request extensions.
#[derive(Debug, Clone)]
pub struct Actor {
    /// Identity from the key spec (`key:name[:role]`).
    pub name: String,
    /// Role tag from the key spec; `operator` when absent.
    pub role: String,
}

impl Actor {
    /// Anonymous identity used in dev mode.
    fn anonymous() -> Self {
        Self {
            name: "anonymous".to_string(),
            role: "operator".to_string(),
        }
    }
}

/// What a key authenticates as.
#[derive(Debug, Clone)]
struct KeyEntry {
    actor: String,
    role: String,
}

/// Configured API keys mapped to the identity they authenticate as.
#[derive(Clone, Default)]
pub struct ApiKeys {
    keys: Arc<HashMap<String, KeyEntry>>,
}

impl ApiKeys {
    /// Reads `ARES_API_KEYS` (`key:actor[:role],...`) then `ARES_API_KEY`.
    pub fn from_env() -> Self {
        let mut keys = HashMap::new();
        if let Ok(spec) = std::env::var("ARES_API_KEYS") {
            for entry in spec.split(',').map(str::trim).filter(|e| !e.is_empty()) {
                let mut parts = entry.splitn(3, ':');
                let key = parts.next().unwrap_or_default().trim();
                if key.is_empty() {
                    continue;
                }
                let actor = parts.next().unwrap_or("operator").trim().to_string();
                let role = parts.next().unwrap_or("operator").trim().to_string();
                keys.insert(key.to_string(), KeyEntry { actor, role });
            }
        }
        if let Ok(key) = std::env::var("ARES_API_KEY") {
            if !key.trim().is_empty() {
                keys.insert(
                    key.trim().to_string(),
                    KeyEntry {
                        actor: "operator".to_string(),
                        role: "operator".to_string(),
                    },
                );
            }
        }
        Self {
            keys: Arc::new(keys),
        }
    }

    /// `from_env` plus production enforcement: when `ARES_REQUIRE_AUTH=1`,
    /// exits the process if no key is configured or any key is shorter than
    /// [`MIN_KEY_LEN`]. Always warns once about weak keys.
    pub fn enforced(self) -> Self {
        static WARNED: OnceLock<()> = OnceLock::new();
        let weak = self.keys.keys().filter(|k| k.len() < MIN_KEY_LEN).count();
        let require = std::env::var("ARES_REQUIRE_AUTH").as_deref() == Ok("1");
        WARNED.get_or_init(|| {
            if self.keys.is_empty() {
                warn!("ARES_API_KEYS unset: running without inbound authentication");
            }
            if weak > 0 {
                warn!("{weak} configured API key(s) shorter than {MIN_KEY_LEN} chars");
            }
        });
        if require && self.keys.is_empty() {
            eprintln!("ARES_REQUIRE_AUTH=1 but ARES_API_KEYS is unset — refusing to start");
            std::process::exit(1);
        }
        if require && weak > 0 {
            eprintln!("ARES_REQUIRE_AUTH=1 but {weak} API key(s) are shorter than {MIN_KEY_LEN} chars — refusing to start");
            std::process::exit(1);
        }
        self
    }

    /// Builds a key set from explicit `(key, actor, role)` triples.
    pub fn new(keys: impl IntoIterator<Item = (String, String, String)>) -> Self {
        Self {
            keys: Arc::new(
                keys.into_iter()
                    .map(|(k, actor, role)| (k, KeyEntry { actor, role }))
                    .collect(),
            ),
        }
    }

    /// `true` when at least one key is configured.
    pub fn is_enabled(&self) -> bool {
        !self.keys.is_empty()
    }

    /// Identity for `token`, when the token is valid.
    pub fn lookup(&self, token: &str) -> Option<Actor> {
        self.keys.get(token).map(|e| Actor {
            name: e.actor.clone(),
            role: e.role.clone(),
        })
    }
}

/// Extracts the presented token from `Authorization: Bearer` or `X-API-Key`.
pub fn presented_token(headers: &axum::http::HeaderMap) -> Option<String> {
    if let Some(value) = headers.get("x-api-key") {
        if let Ok(token) = value.to_str() {
            return Some(token.trim().to_string());
        }
    }
    headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| {
            v.strip_prefix("Bearer ")
                .or_else(|| v.strip_prefix("bearer "))
        })
        .map(|t| t.trim().to_string())
}

/// Rejects unauthenticated requests when keys are configured, and records the
/// caller's [`Actor`] for downstream handlers.
pub async fn require_auth(State(keys): State<ApiKeys>, mut req: Request, next: Next) -> Response {
    if !keys.is_enabled() {
        req.extensions_mut().insert(Actor::anonymous());
        return next.run(req).await;
    }
    let Some(token) = presented_token(req.headers()) else {
        return unauthorized("missing credentials");
    };
    let Some(actor) = keys.lookup(&token) else {
        return unauthorized("invalid credentials");
    };
    req.extensions_mut().insert(actor);
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
    fn key_spec_parses_actor_and_role() {
        let keys = ApiKeys::new([
            (
                "k1".to_string(),
                "alice".to_string(),
                "operator".to_string(),
            ),
            ("k2".to_string(), "bob".to_string(), "approver".to_string()),
        ]);
        assert!(keys.is_enabled());
        assert_eq!(keys.lookup("k1").unwrap().name, "alice");
        assert_eq!(keys.lookup("k2").unwrap().role, "approver");
        assert!(keys.lookup("nope").is_none());
    }

    #[test]
    fn empty_key_set_is_disabled() {
        assert!(!ApiKeys::default().is_enabled());
    }
}

#[cfg(test)]
mod middleware_tests {
    use super::*;
    use axum::body::Body;
    use axum::middleware;
    use axum::routing::get;
    use axum::Router;
    use tower::ServiceExt;

    fn app(keys: ApiKeys) -> Router {
        Router::new()
            .route("/protected", get(|| async { "ok" }))
            .layer(middleware::from_fn_with_state(keys, require_auth))
    }

    fn req(auth: Option<&str>, api_key: Option<&str>) -> Request {
        let mut builder = Request::builder().uri("/protected");
        if let Some(value) = auth {
            builder = builder.header("authorization", value);
        }
        if let Some(value) = api_key {
            builder = builder.header("x-api-key", value);
        }
        builder.body(Body::empty()).unwrap()
    }

    fn keys() -> ApiKeys {
        ApiKeys::new([(
            "k1".to_string(),
            "alice".to_string(),
            "operator".to_string(),
        )])
    }

    #[tokio::test]
    async fn missing_credentials_are_rejected() {
        let res = app(keys()).oneshot(req(None, None)).await.unwrap();
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn wrong_credentials_are_rejected() {
        let res = app(keys())
            .oneshot(req(Some("Bearer nope"), None))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn bearer_and_api_key_both_authenticate() {
        for request in [req(Some("Bearer k1"), None), req(None, Some("k1"))] {
            let res = app(keys()).oneshot(request).await.unwrap();
            assert_eq!(res.status(), StatusCode::OK);
        }
    }

    #[tokio::test]
    async fn dev_mode_without_keys_allows_anonymous() {
        let res = app(ApiKeys::default())
            .oneshot(req(None, None))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
    }
}
