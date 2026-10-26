//! Rate-limit wiring for the api-server surface.
//!
//! Tier assignment by method+path; keys are the peer IP until
//! authentication lands (API-key tiers replace them then). Numbers load
//! from `$POLICIES_DIR/rate-limits.yml`, defaults in [`http_limits`].

use std::{path::PathBuf, sync::Arc};

use axum::{extract::Request, http::Method};
use http_limits::{RateLimiter, RateLimits, Tier};

/// Builds the shared limiter and starts the idle-key purge task.
pub fn build() -> Arc<RateLimiter> {
    let dir = std::env::var("POLICIES_DIR").unwrap_or_else(|_| "policies".to_string());
    let limits = RateLimits::load(&PathBuf::from(dir).join("rate-limits.yml"));
    let rl = Arc::new(RateLimiter::new(limits, classify));
    rl.spawn_idle_purge();
    rl
}

/// Maps a request to its limit tier (see `docs/services.md` rate table).
fn classify(req: &Request) -> Tier {
    let path = req.uri().path();
    if path == "/health" {
        return Tier::Exempt;
    }
    // Long-lived SSE: concurrency-capped, not request-counted.
    if req.method() == Method::GET
        && (path == "/stream/activity"
            || (path.starts_with("/a2a/tasks/") && path.ends_with("/events"))
            || (path.starts_with("/acp/runs/") && path.ends_with("/events")))
    {
        return Tier::Stream;
    }
    if req.method() == Method::POST {
        if path == "/controls/kill-switch" {
            return Tier::KillSwitch;
        }
        if path == "/cvss/score" || path.starts_with("/a2a/") || path.starts_with("/acp/runs") {
            return Tier::Expensive;
        }
    }
    Tier::General
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
    fn classify_maps_paths_to_tiers() {
        assert_eq!(classify(&req(Method::GET, "/health")), Tier::Exempt);
        assert_eq!(classify(&req(Method::GET, "/alerts")), Tier::General);
        assert_eq!(
            classify(&req(Method::GET, "/stream/activity")),
            Tier::Stream
        );
        assert_eq!(
            classify(&req(Method::GET, "/a2a/tasks/abc/events")),
            Tier::Stream
        );
        assert_eq!(classify(&req(Method::POST, "/cvss/score")), Tier::Expensive);
        assert_eq!(
            classify(&req(Method::POST, "/a2a/triage-agent")),
            Tier::Expensive
        );
        assert_eq!(
            classify(&req(Method::POST, "/controls/kill-switch")),
            Tier::KillSwitch
        );
        // GET kill-switch reads state — general read tier.
        assert_eq!(
            classify(&req(Method::GET, "/controls/kill-switch")),
            Tier::General
        );
        // A2A discovery GETs stay in the general tier.
        assert_eq!(classify(&req(Method::GET, "/a2a/agents")), Tier::General);
    }
}
