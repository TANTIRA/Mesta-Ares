//! API-key authentication for the gateway edge.
//!
//! Keys come from `GATEWAY_API_KEYS` (comma-separated). A request
//! authenticates with `Authorization: Bearer <key>` or `X-API-Key: <key>`.
//! With no keys configured the gateway fails closed unless
//! `GATEWAY_ALLOW_ANONYMOUS=1` is set for local development.
//!
//! `/health` and `/metrics` are exempt so probes and scrapers work without a
//! credential.

use std::sync::Arc;

use axum::{
    extract::{Request, State},
    http::{header, HeaderMap, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;
use tracing::warn;

/// Parsed gateway credentials plus the anonymous escape hatch.
#[derive(Clone)]
pub struct AuthState {
    keys: Arc<Vec<String>>,
    allow_anonymous: bool,
}

impl AuthState {
    /// Builds state from `GATEWAY_API_KEYS` and `GATEWAY_ALLOW_ANONYMOUS`.
    pub fn from_env() -> Self {
        let keys = std::env::var("GATEWAY_API_KEYS")
            .unwrap_or_default()
            .split(',')
            .map(str::trim)
            .filter(|k| !k.is_empty())
            .map(str::to_string)
            .collect::<Vec<_>>();
        let allow_anonymous = std::env::var("GATEWAY_ALLOW_ANONYMOUS")
            .map(|v| matches!(v.trim(), "1" | "true" | "yes" | "on"))
            .unwrap_or(false);
        if keys.is_empty() {
            warn!(
                allow_anonymous,
                "GATEWAY_API_KEYS unset; gateway rejects authenticated routes"
            );
        }
        Self {
            keys: Arc::new(keys),
            allow_anonymous,
        }
    }

    /// Number of configured keys (for the boot log).
    pub fn key_count(&self) -> usize {
        self.keys.len()
    }
}

/// Paths that never require a credential.
pub fn is_exempt(path: &str) -> bool {
    path == "/health" || path == "/metrics"
}

/// Extracts the presented credential from either supported header.
pub fn presented_key(headers: &HeaderMap) -> Option<String> {
    if let Some(v) = headers.get("x-api-key").and_then(|v| v.to_str().ok()) {
        let v = v.trim();
        if !v.is_empty() {
            return Some(v.to_string());
        }
    }
    let auth = headers.get(header::AUTHORIZATION)?.to_str().ok()?.trim();
    let (scheme, token) = auth.split_once(' ')?;
    if scheme.eq_ignore_ascii_case("bearer") && !token.trim().is_empty() {
        Some(token.trim().to_string())
    } else {
        None
    }
}

impl AuthState {
    /// `true` when `key` is one of the configured credentials.
    pub fn accepts(&self, key: &str) -> bool {
        // Compare every configured key so timing does not reveal a prefix.
        let mut ok = false;
        for k in self.keys.iter() {
            ok |= k.len() == key.len() && k.as_bytes() == key.as_bytes();
        }
        ok
    }
}

/// Middleware: 401 unless the request carries a valid key (or is exempt).
pub async fn middleware(State(auth): State<AuthState>, req: Request, next: Next) -> Response {
    if is_exempt(req.uri().path()) {
        return next.run(req).await;
    }
    match presented_key(req.headers()) {
        Some(key) if auth.accepts(&key) => next.run(req).await,
        Some(_) => unauthorized("invalid api key"),
        None if auth.keys.is_empty() && auth.allow_anonymous => next.run(req).await,
        None if auth.keys.is_empty() => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({"error": "gateway_auth_not_configured"})),
        )
            .into_response(),
        None => unauthorized("missing api key"),
    }
}

fn unauthorized(message: &str) -> Response {
    (
        StatusCode::UNAUTHORIZED,
        [(header::WWW_AUTHENTICATE, "Bearer")],
        Json(json!({"error": message})),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    fn state(keys: &[&str], allow_anonymous: bool) -> AuthState {
        AuthState {
            keys: Arc::new(keys.iter().map(|k| k.to_string()).collect()),
            allow_anonymous,
        }
    }

    #[test]
    fn bearer_and_api_key_headers_are_read() {
        let mut h = HeaderMap::new();
        h.insert(
            header::AUTHORIZATION,
            HeaderValue::from_static("Bearer abc123"),
        );
        assert_eq!(presented_key(&h).as_deref(), Some("abc123"));

        let mut h = HeaderMap::new();
        h.insert("x-api-key", HeaderValue::from_static("def456"));
        assert_eq!(presented_key(&h).as_deref(), Some("def456"));

        let mut h = HeaderMap::new();
        h.insert(header::AUTHORIZATION, HeaderValue::from_static("Basic zzz"));
        assert_eq!(presented_key(&h), None);
        assert_eq!(presented_key(&HeaderMap::new()), None);
    }

    #[test]
    fn accepts_only_configured_keys() {
        let s = state(&["k1", "k2"], false);
        assert!(s.accepts("k1"));
        assert!(s.accepts("k2"));
        assert!(!s.accepts("k3"));
        assert!(!s.accepts(""));
    }

    #[test]
    fn health_and_metrics_are_exempt() {
        assert!(is_exempt("/health"));
        assert!(is_exempt("/metrics"));
        assert!(!is_exempt("/api/alerts"));
        assert!(!is_exempt("/ingest/wazuh"));
    }

    #[test]
    fn empty_key_list_needs_the_anonymous_flag() {
        assert!(!state(&[], false).allow_anonymous);
        assert!(state(&[], true).allow_anonymous);
    }
}
