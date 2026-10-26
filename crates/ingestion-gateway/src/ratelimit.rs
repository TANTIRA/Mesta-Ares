//! Rate-limit wiring for the ingestion-gateway surface.
//!
//! All webhooks share the `ingest` tier (storm-tolerant: sustained 100/s,
//! burst 500 per key). Keys are the authenticated actor when API keys are
//! configured, else the peer IP.
//! Numbers load from `$POLICIES_DIR/rate-limits.yml`.

use std::sync::Arc;

use axum::extract::Request;
use http_limits::{RateLimiter, Tier};

/// Builds the shared limiter and starts the idle-key purge task.
pub fn build() -> Arc<RateLimiter> {
    http_limits::build_limiter(classify)
}

/// `/health` is unmetered; everything else is an ingest POST.
fn classify(req: &Request) -> Tier {
    if req.uri().path() == "/health" {
        Tier::Exempt
    } else {
        Tier::Ingest
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, http::Method};

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
        assert_eq!(classify(&req(Method::POST, "/webhook/wazuh")), Tier::Ingest);
        assert_eq!(
            classify(&req(Method::POST, "/webhook/generic")),
            Tier::Ingest
        );
    }
}
