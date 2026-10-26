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
    http::header,
    middleware::Next,
    response::{IntoResponse, Response},
};
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
    match http_limits::auth::presented_token(req.headers()) {
        Some(key) if auth.accepts(&key) => next.run(req).await,
        Some(_) => unauthorized("invalid api key"),
        None if auth.keys.is_empty() && auth.allow_anonymous => next.run(req).await,
        None if auth.keys.is_empty() => http_limits::error::ApiError::unavailable(
            "gateway auth not configured (set GATEWAY_API_KEYS)",
        )
        .into_response(),
        None => unauthorized("missing api key"),
    }
}

fn unauthorized(message: &str) -> Response {
    let mut res = http_limits::error::ApiError::unauthorized(message).into_response();
    res.headers_mut().insert(
        header::WWW_AUTHENTICATE,
        axum::http::HeaderValue::from_static("Bearer"),
    );
    res
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::{HeaderMap, HeaderValue};

    fn state(keys: &[&str], allow_anonymous: bool) -> AuthState {
        AuthState {
            keys: Arc::new(keys.iter().map(|k| k.to_string()).collect()),
            allow_anonymous,
        }
    }

    /// The header extractor is shared with every other edge service.
    #[test]
    fn credentials_are_read_by_the_shared_extractor() {
        let mut h = HeaderMap::new();
        h.insert(
            header::AUTHORIZATION,
            HeaderValue::from_static("Bearer abc123"),
        );
        assert_eq!(
            http_limits::auth::presented_token(&h).as_deref(),
            Some("abc123")
        );

        let mut h = HeaderMap::new();
        h.insert("x-api-key", HeaderValue::from_static("def456"));
        assert_eq!(
            http_limits::auth::presented_token(&h).as_deref(),
            Some("def456")
        );

        let mut h = HeaderMap::new();
        h.insert(header::AUTHORIZATION, HeaderValue::from_static("Basic zzz"));
        assert_eq!(http_limits::auth::presented_token(&h), None);
        assert_eq!(http_limits::auth::presented_token(&HeaderMap::new()), None);
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
