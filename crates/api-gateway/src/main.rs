//! API gateway for Ares.
//!
//! Single entry point in front of the three HTTP services:
//!
//! | Path | Upstream |
//! | - | - |
//! | `/api/*` | api-server (console read models, A2A/ACP, vulns) |
//! | `/ingest/*` | ingestion-gateway (alert webhooks) |
//! | `/tools/*` | tool-server (policy-gated tool calls) |
//! | `/aggregate/dashboard` | fan-in of metrics + alerts + advisories |
//! | `/health`, `/metrics` | the gateway itself |
//!
//! Cross-cutting concerns handled here, not in every service:
//! - **Authentication** — API keys (`GATEWAY_API_KEYS`), fails closed when
//!   unset unless `GATEWAY_ALLOW_ANONYMOUS=1`.
//! - **Rate limiting** — `http-limits` token buckets per peer IP and tier,
//!   numbers from `$POLICIES_DIR/rate-limits.yml`.
//! - **Caching** — TTL cache for GETs under `GATEWAY_CACHE_PREFIXES`.
//! - **Monitoring** — per-route status counters and latency sums on
//!   `/metrics` (Prometheus text), one tracing event per request.
//! - **Load distribution** — comma-separated upstream lists, round-robin.
//!
//! Protocol translation is out of scope: every upstream speaks HTTP/JSON.

mod auth;
mod cache;
mod metrics;
mod proxy;

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use axum::{
    extract::{Request, State},
    http::{header, Method, StatusCode},
    middleware,
    response::{IntoResponse, Response},
    routing::{any, get},
    Json, Router,
};
use clap::Parser;
use http_limits::{RateLimiter, RateLimits, Tier};
use serde_json::json;
use tracing::info;

/// Gateway configuration.
#[derive(Debug, Parser)]
#[command(name = "api-gateway", about = "Ares API gateway")]
struct Config {
    /// Address to bind.
    #[arg(long, env = "GATEWAY_BIND", default_value = "0.0.0.0:8082")]
    bind_addr: String,

    /// api-server upstreams (comma-separated, round-robin).
    #[arg(
        long,
        env = "GATEWAY_API_UPSTREAMS",
        default_value = "http://localhost:8080"
    )]
    api_upstreams: String,

    /// ingestion-gateway upstreams.
    #[arg(
        long,
        env = "GATEWAY_INGEST_UPSTREAMS",
        default_value = "http://localhost:8091"
    )]
    ingest_upstreams: String,

    /// tool-server upstreams.
    #[arg(
        long,
        env = "GATEWAY_TOOLS_UPSTREAMS",
        default_value = "http://localhost:8090"
    )]
    tools_upstreams: String,

    /// Cache TTL for cacheable GETs; 0 disables caching.
    #[arg(long, env = "GATEWAY_CACHE_TTL_SECONDS", default_value_t = 30)]
    cache_ttl_seconds: u64,

    /// Path prefixes whose GET responses are cached.
    #[arg(long, env = "GATEWAY_CACHE_PREFIXES", default_value = "/api/vulns")]
    cache_prefixes: String,

    /// Upstream request timeout in seconds.
    #[arg(long, env = "GATEWAY_UPSTREAM_TIMEOUT_SECONDS", default_value_t = 30)]
    upstream_timeout_seconds: u64,
}

/// Splits a comma-separated env value into trimmed, non-empty entries.
fn split_list(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

struct AppState {
    http: reqwest::Client,
    api_upstreams: Vec<String>,
    ingest_upstreams: Vec<String>,
    tools_upstreams: Vec<String>,
    rr: proxy::RoundRobin,
    cache: cache::ResponseCache,
    metrics: metrics::Metrics,
}

/// Maps a gateway request to a rate-limit tier.
fn classify(req: &Request) -> Tier {
    let path = req.uri().path();
    if auth::is_exempt(path) {
        return Tier::Exempt;
    }
    if path.starts_with("/ingest") {
        return Tier::Ingest;
    }
    if path.starts_with("/tools") {
        return Tier::Expensive;
    }
    if req.method() == Method::GET && (path == "/api/stream/activity" || path.ends_with("/events"))
    {
        return Tier::Stream;
    }
    Tier::General
}

async fn health() -> impl IntoResponse {
    Json(json!({"status": "ok", "service": "api-gateway"}))
}

async fn metrics(State(s): State<Arc<AppState>>) -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "text/plain; version=0.0.4")],
        s.metrics.render(),
    )
}

/// `GET /aggregate/dashboard` entry point.
async fn aggregate(State(s): State<Arc<AppState>>) -> Response {
    proxy::aggregate(State(s)).await
}

fn router(state: Arc<AppState>, auth: auth::AuthState, rl: Arc<RateLimiter>) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/metrics", get(metrics))
        .route("/aggregate/dashboard", get(aggregate))
        .route("/api/{*path}", any(proxy::api))
        .route("/ingest/{*path}", any(proxy::ingest))
        .route("/tools/{*path}", any(proxy::tools))
        .fallback(|| async { (StatusCode::NOT_FOUND, Json(json!({"error": "no_route"}))) })
        // Rate limiting outermost so unauthenticated floods are still metered;
        // authentication then gates everything except the exempt paths.
        .layer(middleware::from_fn_with_state(rl, http_limits::middleware))
        .layer(middleware::from_fn_with_state(auth, auth::middleware))
        .with_state(state)
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,api_gateway=debug".into()),
        )
        .init();

    let cfg = Config::parse();
    let policies_dir = std::env::var("POLICIES_DIR").unwrap_or_else(|_| "policies".to_string());
    let limits = RateLimits::load(&std::path::PathBuf::from(&policies_dir).join("rate-limits.yml"));
    let rl = Arc::new(RateLimiter::new(limits, classify));
    rl.spawn_idle_purge();

    let auth = auth::AuthState::from_env();
    if auth.key_count() == 0 {
        tracing::warn!("no GATEWAY_API_KEYS configured — see GATEWAY_ALLOW_ANONYMOUS for dev");
    }

    let state = Arc::new(AppState {
        http: reqwest::Client::builder()
            .timeout(Duration::from_secs(cfg.upstream_timeout_seconds))
            .build()
            .context("build upstream client")?,
        api_upstreams: split_list(&cfg.api_upstreams),
        ingest_upstreams: split_list(&cfg.ingest_upstreams),
        tools_upstreams: split_list(&cfg.tools_upstreams),
        rr: proxy::RoundRobin::default(),
        cache: cache::ResponseCache::new(
            Duration::from_secs(cfg.cache_ttl_seconds),
            split_list(&cfg.cache_prefixes),
        ),
        metrics: metrics::Metrics::default(),
    });

    let app = router(state, auth, rl);
    let listener = tokio::net::TcpListener::bind(&cfg.bind_addr).await?;
    info!(addr = %cfg.bind_addr, "api-gateway listening");
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;

    fn req(method: Method, uri: &str) -> Request {
        Request::builder()
            .method(method)
            .uri(uri)
            .body(Body::empty())
            .unwrap()
    }

    #[test]
    fn classify_maps_gateway_paths_to_tiers() {
        assert_eq!(classify(&req(Method::GET, "/health")), Tier::Exempt);
        assert_eq!(classify(&req(Method::GET, "/metrics")), Tier::Exempt);
        assert_eq!(classify(&req(Method::POST, "/ingest/wazuh")), Tier::Ingest);
        assert_eq!(
            classify(&req(Method::POST, "/tools/invoke")),
            Tier::Expensive
        );
        assert_eq!(
            classify(&req(Method::GET, "/api/stream/activity")),
            Tier::Stream
        );
        assert_eq!(
            classify(&req(Method::GET, "/api/a2a/tasks/1/events")),
            Tier::Stream
        );
        assert_eq!(classify(&req(Method::GET, "/api/alerts")), Tier::General);
        assert_eq!(
            classify(&req(Method::GET, "/aggregate/dashboard")),
            Tier::General
        );
    }

    #[test]
    fn split_list_trims_and_drops_empties() {
        assert_eq!(
            split_list(" http://a:1 , ,http://b:2 "),
            vec!["http://a:1".to_string(), "http://b:2".to_string()]
        );
        assert!(split_list("  ").is_empty());
    }

    /// Route registration panics on bad path syntax (axum 0.8 requires
    /// `{param}` captures); building the router proves it is well-formed.
    #[test]
    fn router_registers_all_routes() {
        let state = Arc::new(AppState {
            http: reqwest::Client::new(),
            api_upstreams: vec!["http://localhost:8080".into()],
            ingest_upstreams: vec!["http://localhost:8091".into()],
            tools_upstreams: vec!["http://localhost:8090".into()],
            rr: proxy::RoundRobin::default(),
            cache: cache::ResponseCache::new(Duration::ZERO, vec![]),
            metrics: metrics::Metrics::default(),
        });
        let auth = auth::AuthState::from_env();
        let rl = Arc::new(RateLimiter::new(RateLimits::default(), classify));
        let _ = router(state, auth, rl);
    }
}

#[cfg(test)]
mod spec_tests {
    /// Every OpenAPI file parses and every internal `$ref` resolves.
    ///
    /// A dangling `$ref` is otherwise silent until someone opens the docs.
    #[test]
    fn specs_are_valid_openapi() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/openapi");
        let mut checked = 0;
        for entry in std::fs::read_dir(&dir).expect("docs/openapi readable") {
            let path = entry.expect("dir entry").path();
            if path.extension().and_then(|e| e.to_str()) != Some("yaml") {
                continue;
            }
            let text = std::fs::read_to_string(&path).expect("spec readable");
            let doc: serde_yaml_ng::Value = serde_yaml_ng::from_str(&text)
                .unwrap_or_else(|e| panic!("{}: {e}", path.display()));

            // `redocly.yaml` is the lint ruleset, not a spec.
            let Some(version) = doc["openapi"].as_str() else {
                continue;
            };
            assert!(
                version.starts_with("3.0") || version.starts_with("3.1"),
                "{}: openapi must be 3.0+, got {version:?}",
                path.display()
            );
            for key in ["info", "paths"] {
                assert!(
                    doc.get(key).is_some(),
                    "{}: missing top-level `{key}`",
                    path.display()
                );
            }
            let paths = doc["paths"].as_mapping().expect("paths is a mapping");
            for (name, _) in paths {
                let p = name.as_str().unwrap_or_default();
                assert!(
                    p.starts_with('/'),
                    "{}: path `{p}` must start with /",
                    path.display()
                );
            }

            let mut refs = Vec::new();
            collect_refs(&doc, &mut refs);
            for r in refs {
                assert!(
                    resolve_ref(&doc, &r),
                    "{}: unresolved $ref `{r}`",
                    path.display()
                );
            }
            checked += 1;
        }
        assert!(
            checked >= 4,
            "expected the four service specs, found {checked}"
        );
    }

    fn collect_refs(value: &serde_yaml_ng::Value, out: &mut Vec<String>) {
        match value {
            serde_yaml_ng::Value::String(s) if s.starts_with("#/") => out.push(s.clone()),
            serde_yaml_ng::Value::Mapping(m) => {
                for (_, v) in m {
                    collect_refs(v, out);
                }
            }
            serde_yaml_ng::Value::Sequence(s) => {
                for v in s {
                    collect_refs(v, out);
                }
            }
            _ => {}
        }
    }

    /// Resolves a local `#/a/b/c` pointer.
    fn resolve_ref(doc: &serde_yaml_ng::Value, pointer: &str) -> bool {
        let mut cur = doc;
        for segment in pointer.trim_start_matches("#/").split('/') {
            match cur.get(segment) {
                Some(next) => cur = next,
                None => return false,
            }
        }
        true
    }
}
