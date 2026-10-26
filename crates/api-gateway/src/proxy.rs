//! Routing, round-robin selection and request forwarding.
//!
//! Three upstream groups are exposed under path prefixes:
//!
//! | Prefix | Upstream | Env |
//! | - | - | - |
//! | `/api/*` | api-server | `GATEWAY_API_UPSTREAMS` |
//! | `/ingest/*` | ingestion-gateway | `GATEWAY_INGEST_UPSTREAMS` |
//! | `/tools/*` | tool-server | `GATEWAY_TOOLS_UPSTREAMS` |
//!
//! Each env var takes a comma-separated list; requests rotate across the
//! list. Response bodies are streamed, so SSE endpoints keep working.
//!
//! Protocol translation (HTTP/JSON ⇄ gRPC) is deliberately absent — every
//! upstream speaks HTTP/JSON.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Instant;

use axum::{
    body::{Body, Bytes},
    extract::{Path, Request, State},
    http::{header, HeaderMap, HeaderName, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde_json::{json, Value};
use tracing::warn;

use crate::AppState;

/// Largest request body the gateway will buffer.
const MAX_REQUEST_BODY: usize = 8 * 1024 * 1024;

/// Upstream group a path prefix routes to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Route {
    /// Console/read API (`/api/*`).
    Api,
    /// Alert webhooks (`/ingest/*`).
    Ingest,
    /// Policy-gated tool calls (`/tools/*`).
    Tools,
}

impl Route {
    /// Metrics label for this route.
    pub fn label(self) -> &'static str {
        match self {
            Route::Api => "api",
            Route::Ingest => "ingest",
            Route::Tools => "tools",
        }
    }

    fn index(self) -> usize {
        match self {
            Route::Api => 0,
            Route::Ingest => 1,
            Route::Tools => 2,
        }
    }
}

/// Round-robin cursor per route.
#[derive(Default)]
pub struct RoundRobin {
    counters: [AtomicUsize; 3],
}

impl RoundRobin {
    /// Picks the next upstream index for `route`, wrapping at `len`.
    pub fn next(&self, route: Route, len: usize) -> usize {
        if len <= 1 {
            return 0;
        }
        self.counters[route.index()].fetch_add(1, Ordering::Relaxed) % len
    }
}

/// Headers that must not be copied between hops.
fn is_hop_by_hop(name: &HeaderName) -> bool {
    matches!(
        name.as_str(),
        "connection"
            | "keep-alive"
            | "proxy-authenticate"
            | "proxy-authorization"
            | "te"
            | "trailer"
            | "transfer-encoding"
            | "upgrade"
            | "host"
            | "content-length"
    )
}

/// Builds the upstream URL for `path` (+ optional query).
fn upstream_url(base: &str, path: &str, query: Option<&str>) -> String {
    let base = base.trim_end_matches('/');
    match query {
        Some(q) if !q.is_empty() => format!("{base}/{path}?{q}"),
        _ => format!("{base}/{path}"),
    }
}

/// `GET /aggregate/dashboard` payload from per-source results.
///
/// Each source contributes its body under `sources.<name>`; a failed source
/// contributes `{"error": "..."}` instead of failing the whole response.
pub fn aggregate_payload(sources: Vec<(&str, Result<Value, String>)>) -> Value {
    let mut map = serde_json::Map::new();
    for (name, result) in sources {
        let value = match result {
            Ok(v) => v,
            Err(e) => json!({ "error": e }),
        };
        map.insert(name.to_string(), value);
    }
    json!({
        "generated_at": chrono::Utc::now(),
        "sources": Value::Object(map),
    })
}

/// Shared forwarding path for all three prefixes.
async fn forward(s: Arc<AppState>, route: Route, path: String, req: Request) -> Response {
    let started = Instant::now();
    let method = req.method().clone();
    let query = req.uri().query().map(str::to_string);
    let headers = req.headers().clone();
    let body = match axum::body::to_bytes(req.into_body(), MAX_REQUEST_BODY).await {
        Ok(b) => b,
        Err(_) => {
            return (
                StatusCode::PAYLOAD_TOO_LARGE,
                Json(json!({"error": "request_body_too_large"})),
            )
                .into_response()
        }
    };

    let bases = match route {
        Route::Api => &s.api_upstreams,
        Route::Ingest => &s.ingest_upstreams,
        Route::Tools => &s.tools_upstreams,
    };
    if bases.is_empty() {
        return (
            StatusCode::BAD_GATEWAY,
            Json(json!({"error": "no upstream configured", "route": route.label()})),
        )
            .into_response();
    }
    let base = &bases[s.rr.next(route, bases.len())];
    let url = upstream_url(base, &path, query.as_deref());

    // Cache lookup for cacheable GETs.
    let cache_key = format!("{path}?{}", query.clone().unwrap_or_default());
    let cacheable = s.cache.applies(method.as_str(), &format!("/{path}"));
    if cacheable {
        if let Some(hit) = s.cache.get(&cache_key) {
            s.metrics.hit();
            s.metrics.record(route.label(), hit.status, started.elapsed());
            return cached_response(hit);
        }
    }

    let mut rb = s.http.request(method.clone(), &url);
    for (name, value) in headers.iter() {
        if is_hop_by_hop(name) {
            continue;
        }
        // Never forward the gateway credential to a backend.
        if name == "x-api-key" {
            continue;
        }
        rb = rb.header(name, value);
    }
    let sent = rb.body(body.to_vec()).send().await;

    let resp = match sent {
        Ok(r) => r,
        Err(e) => {
            warn!(route = route.label(), upstream = %url, error = %e, "upstream request failed");
            s.metrics.record(route.label(), 502, started.elapsed());
            return (
                StatusCode::BAD_GATEWAY,
                Json(json!({"error": "upstream_unavailable", "upstream": url})),
            )
                .into_response();
        }
    };

    let status = resp.status();
    let mut out = Response::builder().status(status);
    for (name, value) in resp.headers().iter() {
        if is_hop_by_hop(name) {
            continue;
        }
        out = out.header(name, value);
    }
    let content_type = resp
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);

    s.metrics
        .record(route.label(), status.as_u16(), started.elapsed());

    if cacheable && status == StatusCode::OK {
        let bytes: Bytes = match resp.bytes().await {
            Ok(b) => b,
            Err(e) => {
                warn!(error = %e, "failed reading upstream body for cache");
                return StatusCode::BAD_GATEWAY.into_response();
            }
        };
        s.cache
            .put(&cache_key, status.as_u16(), content_type, &bytes);
        return out
            .header("x-gateway-cache", "miss")
            .body(Body::from(bytes))
            .unwrap_or_else(|_| StatusCode::BAD_GATEWAY.into_response());
    }

    out.body(Body::from_stream(resp.bytes_stream()))
        .unwrap_or_else(|_| StatusCode::BAD_GATEWAY.into_response())
}

fn cached_response(hit: crate::cache::Cached) -> Response {
    let mut out = Response::builder()
        .status(hit.status)
        .header("x-gateway-cache", "hit");
    if let Some(ct) = hit.content_type {
        out = out.header(header::CONTENT_TYPE, ct);
    }
    out.body(Body::from(hit.body))
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}

/// `* /api/{*path}` — console/read API.
pub async fn api(
    State(s): State<Arc<AppState>>,
    Path(path): Path<String>,
    req: Request,
) -> Response {
    forward(s, Route::Api, path, req).await
}

/// `* /ingest/{*path}` — alert webhooks.
pub async fn ingest(
    State(s): State<Arc<AppState>>,
    Path(path): Path<String>,
    req: Request,
) -> Response {
    forward(s, Route::Ingest, path, req).await
}

/// `* /tools/{*path}` — tool-server.
pub async fn tools(
    State(s): State<Arc<AppState>>,
    Path(path): Path<String>,
    req: Request,
) -> Response {
    forward(s, Route::Tools, path, req).await
}

/// `GET /aggregate/dashboard` — one call for the console's landing view:
/// metrics + recent alerts + vulnerability advisories, fetched concurrently.
pub async fn aggregate(State(s): State<Arc<AppState>>) -> Response {
    let started = Instant::now();
    let base = match s.api_upstreams.first() {
        Some(b) => b.trim_end_matches('/').to_string(),
        None => {
            return (
                StatusCode::BAD_GATEWAY,
                Json(json!({"error": "no api upstream configured"})),
            )
                .into_response()
        }
    };
    let fetch = |path: &'static str| {
        let s = s.clone();
        let url = format!("{base}{path}");
        async move {
            let res = s.http.get(&url).send().await;
            match res {
                Ok(r) if r.status().is_success() => match r.json::<Value>().await {
                    Ok(v) => Ok(v),
                    Err(e) => Err(format!("decode: {e}")),
                },
                Ok(r) => Err(format!("status {}", r.status().as_u16())),
                Err(e) => Err(format!("request: {e}")),
            }
        }
    };
    let (metrics, alerts, advisories) = tokio::join!(
        fetch("/metrics"),
        fetch("/alerts?limit=10"),
        fetch("/vulns/advisories")
    );
    s.metrics.record("aggregate", 200, started.elapsed());
    Json(aggregate_payload(vec![
        ("metrics", metrics),
        ("alerts", alerts),
        ("advisories", advisories),
    ]))
    .into_response()
}

/// Headers used by tests to assert hop-by-hop stripping.
#[cfg(test)]
fn stripped(headers: &HeaderMap) -> Vec<String> {
    headers
        .keys()
        .filter(|k| is_hop_by_hop(k))
        .map(|k| k.to_string())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upstream_url_joins_path_and_query() {
        assert_eq!(
            upstream_url("http://api:8080/", "alerts", Some("limit=5")),
            "http://api:8080/alerts?limit=5"
        );
        assert_eq!(
            upstream_url("http://api:8080", "vulns/advisories", None),
            "http://api:8080/vulns/advisories"
        );
        assert_eq!(
            upstream_url("http://api:8080", "alerts", Some("")),
            "http://api:8080/alerts"
        );
    }

    #[test]
    fn round_robin_rotates_and_wraps() {
        let rr = RoundRobin::default();
        assert_eq!(rr.next(Route::Api, 3), 0);
        assert_eq!(rr.next(Route::Api, 3), 1);
        assert_eq!(rr.next(Route::Api, 3), 2);
        assert_eq!(rr.next(Route::Api, 3), 0);
        // Independent cursor per route.
        assert_eq!(rr.next(Route::Tools, 3), 0);
        // Single upstream never advances.
        assert_eq!(rr.next(Route::Ingest, 1), 0);
        assert_eq!(rr.next(Route::Ingest, 1), 0);
    }

    #[test]
    fn hop_by_hop_headers_are_detected() {
        let mut h = HeaderMap::new();
        h.insert(header::HOST, HeaderValue::from_static("gw"));
        h.insert("connection", HeaderValue::from_static("keep-alive"));
        h.insert("x-request-id", HeaderValue::from_static("1"));
        let mut names = stripped(&h);
        names.sort();
        assert_eq!(names, vec!["connection".to_string(), "host".to_string()]);
    }

    #[test]
    fn aggregate_keeps_partial_results() {
        let payload = aggregate_payload(vec![
            ("metrics", Ok(json!({"alerts_per_hour": 3}))),
            ("alerts", Err("status 502".to_string())),
        ]);
        assert_eq!(payload["sources"]["metrics"]["alerts_per_hour"], 3);
        assert_eq!(payload["sources"]["alerts"]["error"], "status 502");
        assert!(payload["generated_at"].is_string());
    }
}
