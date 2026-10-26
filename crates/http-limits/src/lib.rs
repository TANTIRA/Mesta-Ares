//! Per-key HTTP rate limiting for the Ares edge services.
//!
//! In-process, dependency-light limiter shared by `api-server` and
//! `ingestion-gateway`. Requests are metered by count through per-key token
//! buckets (`burst` capacity refilled at `per_second`); long-lived SSE
//! streams skip request counting and are instead capped on concurrent
//! connections per key.
//!
//! Keys are the peer IP (`axum::extract::ConnectInfo`) until authentication
//! lands; requests without connect info share one fallback bucket. Limits
//! load from `$POLICIES_DIR/rate-limits.yml` with built-in defaults.
//!
//! Responses carry the legacy trio `x-ratelimit-limit` (bucket capacity /
//! stream cap), `x-ratelimit-remaining`, `x-ratelimit-reset` (epoch seconds
//! when the bucket refills; "now" for streams). Rejection is HTTP 429 with
//! `Retry-After: <seconds>` and `{"error":"rate_limit_exceeded"}`.

use std::{
    io::ErrorKind,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    path::Path,
    sync::Arc,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use axum::{
    body::Body,
    extract::{ConnectInfo, Request, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use dashmap::DashMap;
use futures::StreamExt;
use serde::Deserialize;
use serde_json::json;
use tokio::sync::Semaphore;
use tracing::warn;

/// Limit tier a request classifies into. Part of the bucket map key so the
/// same peer IP gets an independent bucket per tier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Tier {
    /// Unmetered (health checks). No headers emitted.
    Exempt,
    /// General read traffic.
    General,
    /// Expensive endpoints (LLM-backed RPC, CVSS scoring).
    Expensive,
    /// `POST /controls/kill-switch` — tight but never fully closed.
    KillSwitch,
    /// Ingestion webhooks (storm-tolerant).
    Ingest,
    /// Long-lived streams — concurrency-capped, not request-counted.
    Stream,
}

/// Token-bucket parameters: `burst` capacity refilled at `per_second`.
#[derive(Debug, Clone, Copy, Deserialize)]
pub struct TierSpec {
    /// Sustained refill rate in requests per second.
    pub per_second: u32,
    /// Bucket capacity — the maximum instantaneous burst.
    pub burst: u32,
}

/// Loaded limits. `#[serde(default)]` fills missing keys from [`Default`].
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct RateLimits {
    /// [`Tier::General`].
    pub general: TierSpec,
    /// [`Tier::Expensive`].
    pub expensive: TierSpec,
    /// [`Tier::KillSwitch`].
    pub kill_switch: TierSpec,
    /// [`Tier::Ingest`].
    pub ingest: TierSpec,
    /// Max concurrent [`Tier::Stream`] connections per key.
    pub sse_max_concurrent: u32,
}

impl Default for RateLimits {
    fn default() -> Self {
        Self {
            general: TierSpec { per_second: 60, burst: 120 },
            expensive: TierSpec { per_second: 10, burst: 20 },
            kill_switch: TierSpec { per_second: 1, burst: 5 },
            ingest: TierSpec { per_second: 100, burst: 500 },
            sse_max_concurrent: 20,
        }
    }
}

impl RateLimits {
    /// Loads limits from a YAML file; falls back to [`Default`] when the file
    /// is missing or fails to parse (boot never fails on policy drift).
    pub fn load(path: &Path) -> Self {
        match std::fs::read_to_string(path) {
            Ok(text) => match serde_yaml::from_str(&text) {
                Ok(limits) => limits,
                Err(e) => {
                    warn!(error = %e, path = %path.display(),
                        "invalid rate-limits.yml; using defaults");
                    Self::default()
                }
            },
            Err(e) if e.kind() == ErrorKind::NotFound => Self::default(),
            Err(e) => {
                warn!(error = %e, path = %path.display(),
                    "unreadable rate-limits.yml; using defaults");
                Self::default()
            }
        }
    }

    /// Token-bucket spec for a metered tier.
    fn spec(&self, tier: Tier) -> TierSpec {
        match tier {
            Tier::General => self.general,
            Tier::Expensive => self.expensive,
            Tier::KillSwitch => self.kill_switch,
            Tier::Ingest => self.ingest,
            // Exempt/Stream never reach this; return a deny-nothing spec.
            _ => TierSpec { per_second: u32::MAX, burst: u32::MAX },
        }
    }
}

/// One peer's token bucket.
struct Bucket {
    /// Available tokens (fractional — refills continuously).
    tokens: f64,
    /// Last refill/activity instant; drives both refill and idle purge.
    last_seen: Instant,
}

/// Result of a bucket check.
enum Outcome {
    /// Token consumed.
    Allowed {
        /// Whole tokens left after this request.
        remaining: u32,
        /// Epoch seconds when the bucket refills to capacity.
        reset: u64,
    },
    /// Bucket empty; request rejected.
    Limited {
        /// Seconds until at least one token is available.
        retry_after: u64,
        /// Epoch seconds when the bucket refills to capacity.
        reset: u64,
    },
}

/// Shared limiter state: per-(tier, key) token buckets plus per-key stream
/// semaphores, and the caller-supplied path classifier.
pub struct RateLimiter {
    limits: RateLimits,
    classify: fn(&Request) -> Tier,
    buckets: DashMap<(Tier, IpAddr), Bucket>,
    streams: DashMap<IpAddr, Arc<Semaphore>>,
}

impl RateLimiter {
    /// Builds a limiter. `classify` maps each request to a [`Tier`].
    pub fn new(limits: RateLimits, classify: fn(&Request) -> Tier) -> Self {
        Self {
            limits,
            classify,
            buckets: DashMap::new(),
            streams: DashMap::new(),
        }
    }

    /// Spawns the idle-key purge loop (every 60s, drops bucket keys idle
    /// longer than 5 min and stream semaphores no permit-holder references)
    /// so per-key state does not leak memory. Call once at boot.
    pub fn spawn_idle_purge(self: &Arc<Self>) {
        let rl = Arc::clone(self);
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(Duration::from_secs(60));
            tick.tick().await; // first tick fires immediately; skip it
            loop {
                tick.tick().await;
                rl.purge_idle(Duration::from_secs(300));
            }
        });
    }

    /// Removes bucket entries idle for `ttl` and semaphores with no live
    /// permits (`Arc::strong_count > 1` means a permit holder exists).
    fn purge_idle(&self, ttl: Duration) {
        let now = Instant::now();
        self.buckets
            .retain(|_, b| now.duration_since(b.last_seen) < ttl);
        self.streams.retain(|_, s| Arc::strong_count(s) > 1);
    }

    /// Consumes one token from `key`'s bucket in `tier`.
    fn check(&self, tier: Tier, key: IpAddr, spec: TierSpec, now: Instant) -> Outcome {
        let mut entry = self.buckets.entry((tier, key)).or_insert(Bucket {
            tokens: spec.burst as f64,
            last_seen: now,
        });
        let rate = spec.per_second as f64;
        let cap = spec.burst as f64;
        entry.tokens = (entry.tokens + now.duration_since(entry.last_seen).as_secs_f64() * rate)
            .min(cap);
        entry.last_seen = now;
        let epoch_now = epoch_secs();
        // `as u64` saturates on inf/NaN (rate or cap zero) — defined behavior.
        let reset = epoch_now + ((cap - entry.tokens) / rate).ceil() as u64;
        if entry.tokens >= 1.0 {
            entry.tokens -= 1.0;
            Outcome::Allowed {
                remaining: entry.tokens.floor() as u32,
                reset,
            }
        } else {
            Outcome::Limited {
                retry_after: ((1.0 - entry.tokens) / rate).ceil().max(1.0) as u64,
                reset,
            }
        }
    }
}

/// Epoch seconds — the `x-ratelimit-reset` convention.
fn epoch_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Writes the legacy `x-ratelimit-*` trio.
fn set_headers(headers: &mut HeaderMap, limit: u32, remaining: u32, reset: u64) {
    headers.insert("x-ratelimit-limit", HeaderValue::from(limit));
    headers.insert("x-ratelimit-remaining", HeaderValue::from(remaining));
    headers.insert("x-ratelimit-reset", HeaderValue::from(reset));
}

/// Builds the 429 response: `Retry-After` delay-seconds plus the JSON error.
fn limited_response(limit: u32, retry_after: u64, reset: u64) -> Response {
    let mut res = (
        StatusCode::TOO_MANY_REQUESTS,
        Json(json!({"error": "rate_limit_exceeded", "retry_after": retry_after})),
    )
        .into_response();
    let headers = res.headers_mut();
    headers.insert(header::RETRY_AFTER, HeaderValue::from(retry_after));
    set_headers(headers, limit, 0, reset);
    res
}

/// axum `from_fn_with_state` middleware. Mount once per router:
///
/// ```ignore
/// .layer(axum::middleware::from_fn_with_state(rl, http_limits::middleware))
/// ```
///
/// and serve with `into_make_service_with_connect_info::<SocketAddr>()` so
/// peer IPs are available.
pub async fn middleware(
    State(rl): State<Arc<RateLimiter>>,
    req: Request,
    next: Next,
) -> Response {
    let tier = (rl.classify)(&req);
    if tier == Tier::Exempt {
        return next.run(req).await;
    }
    let key = req
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|c| c.0.ip())
        // No connect info (tests, unix sockets): one shared fallback bucket.
        .unwrap_or(IpAddr::V4(Ipv4Addr::UNSPECIFIED));

    if tier == Tier::Stream {
        let max = rl.limits.sse_max_concurrent as usize;
        let sem = rl
            .streams
            .entry(key)
            .or_insert_with(|| Arc::new(Semaphore::new(max)))
            .clone();
        let Ok(permit) = sem.clone().try_acquire_owned() else {
            return limited_response(rl.limits.sse_max_concurrent, 1, epoch_secs());
        };
        let res = next.run(req).await;
        let remaining = sem.available_permits() as u32;
        // Hold the permit inside the response body so it is released only
        // when the stream ends or the connection drops — a `Semaphore` held
        // by the middleware alone would release when the handler returns.
        let (parts, body) = res.into_parts();
        let guarded = futures::stream::unfold((body.into_data_stream(), permit), |(mut s, p)| {
            async move { s.next().await.map(|item| (item, (s, p))) }
        });
        let mut res = Response::from_parts(parts, Body::from_stream(guarded));
        set_headers(
            res.headers_mut(),
            rl.limits.sse_max_concurrent,
            remaining,
            epoch_secs(),
        );
        return res;
    }

    let spec = rl.limits.spec(tier);
    match rl.check(tier, key, spec, Instant::now()) {
        Outcome::Allowed { remaining, reset } => {
            let mut res = next.run(req).await;
            set_headers(res.headers_mut(), spec.burst, remaining, reset);
            res
        }
        Outcome::Limited { retry_after, reset } => {
            limited_response(spec.burst, retry_after, reset)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::to_bytes, routing::get, Router};
    use std::net::{Ipv4Addr, SocketAddrV4};
    use tower::ServiceExt;

    fn peer_req(uri: &str) -> Request {
        let mut r = Request::builder()
            .uri(uri)
            .body(Body::empty())
            .unwrap();
        r.extensions_mut().insert(ConnectInfo(SocketAddr::V4(
            SocketAddrV4::new(Ipv4Addr::LOCALHOST, 4321),
        )));
        r
    }

    fn app(rl: Arc<RateLimiter>) -> Router {
        Router::new()
            .route("/ping", get(|| async { "pong" }))
            .layer(axum::middleware::from_fn_with_state(rl, middleware))
    }

    #[test]
    fn bucket_allows_burst_then_limits() {
        let rl = RateLimiter::new(RateLimits::default(), |_| Tier::General);
        let spec = TierSpec { per_second: 1, burst: 2 };
        let key = IpAddr::V4(Ipv4Addr::LOCALHOST);
        let now = Instant::now();
        assert!(matches!(
            rl.check(Tier::General, key, spec, now),
            Outcome::Allowed { remaining: 1, .. }
        ));
        assert!(matches!(
            rl.check(Tier::General, key, spec, now),
            Outcome::Allowed { remaining: 0, .. }
        ));
        match rl.check(Tier::General, key, spec, now) {
            Outcome::Limited { retry_after, .. } => assert!(retry_after >= 1),
            Outcome::Allowed { .. } => panic!("third request should be limited"),
        }
        // A different tier gets its own bucket.
        assert!(matches!(
            rl.check(Tier::Expensive, key, spec, now),
            Outcome::Allowed { .. }
        ));
    }

    #[test]
    fn bucket_refills_over_time() {
        let rl = RateLimiter::new(RateLimits::default(), |_| Tier::General);
        let spec = TierSpec { per_second: 10, burst: 1 };
        let key = IpAddr::V4(Ipv4Addr::LOCALHOST);
        let t0 = Instant::now();
        assert!(matches!(
            rl.check(Tier::General, key, spec, t0),
            Outcome::Allowed { .. }
        ));
        // 200ms later: 2 tokens refilled, capped at burst=1.
        assert!(matches!(
            rl.check(Tier::General, key, spec, t0 + Duration::from_millis(200)),
            Outcome::Allowed { remaining: 0, .. }
        ));
    }

    #[tokio::test]
    async fn middleware_allows_then_429s_with_headers() {
        let mut limits = RateLimits::default();
        limits.general = TierSpec { per_second: 1, burst: 2 };
        let app = app(Arc::new(RateLimiter::new(limits, |_| Tier::General)));

        for _ in 0..2 {
            let res = app.clone().oneshot(peer_req("/ping")).await.unwrap();
            assert_eq!(res.status(), StatusCode::OK);
            assert!(res.headers().contains_key("x-ratelimit-limit"));
            assert!(res.headers().contains_key("x-ratelimit-remaining"));
            assert!(res.headers().contains_key("x-ratelimit-reset"));
        }
        let res = app.oneshot(peer_req("/ping")).await.unwrap();
        assert_eq!(res.status(), StatusCode::TOO_MANY_REQUESTS);
        assert!(res.headers().get(header::RETRY_AFTER).is_some());
        let body = to_bytes(res.into_body(), usize::MAX).await.unwrap();
        let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(v["error"], "rate_limit_exceeded");
        assert!(v["retry_after"].as_u64().unwrap() >= 1);
    }

    #[tokio::test]
    async fn exempt_tier_passes_through_unmetered() {
        let rl = Arc::new(RateLimiter::new(
            RateLimits {
                general: TierSpec { per_second: 0, burst: 0 },
                ..RateLimits::default()
            },
            |req| {
                if req.uri().path() == "/ping" {
                    Tier::Exempt
                } else {
                    Tier::General
                }
            },
        ));
        let app = app(rl);
        for _ in 0..5 {
            let res = app.clone().oneshot(peer_req("/ping")).await.unwrap();
            assert_eq!(res.status(), StatusCode::OK);
            assert!(res.headers().get("x-ratelimit-limit").is_none());
        }
    }

    #[tokio::test]
    async fn stream_tier_caps_concurrency() {
        let mut limits = RateLimits::default();
        limits.sse_max_concurrent = 2;
        let rl = RateLimiter::new(limits, |_| Tier::Stream);
        let key = IpAddr::V4(Ipv4Addr::LOCALHOST);
        let sem = rl
            .streams
            .entry(key)
            .or_insert_with(|| Arc::new(Semaphore::new(2)))
            .clone();
        let p1 = sem.clone().try_acquire_owned().unwrap();
        let p2 = sem.clone().try_acquire_owned().unwrap();
        assert!(sem.clone().try_acquire_owned().is_err());
        drop(p1);
        drop(p2);
        drop(sem); // map entry is now the only Arc — purge may drop it
        rl.purge_idle(Duration::ZERO);
        assert!(rl.streams.is_empty());
    }
}
