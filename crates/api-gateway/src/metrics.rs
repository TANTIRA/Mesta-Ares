//! Request counters and latency sums, rendered as Prometheus text.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use dashmap::DashMap;

/// Gateway metrics.
#[derive(Default)]
pub struct Metrics {
    /// `(route, status)` → request count.
    requests: DashMap<(String, u16), u64>,
    /// `route` → (latency nanos, count).
    latency: DashMap<String, (u64, u64)>,
    /// Cache hits served without touching an upstream.
    cache_hits: AtomicU64,
}

impl Metrics {
    /// Records one finished request.
    pub fn record(&self, route: &str, status: u16, elapsed: Duration) {
        *self
            .requests
            .entry((route.to_string(), status))
            .or_insert(0) += 1;
        let mut entry = self.latency.entry(route.to_string()).or_insert((0, 0));
        entry.0 += elapsed.as_nanos() as u64;
        entry.1 += 1;
    }

    /// Counts one cache hit.
    pub fn hit(&self) {
        self.cache_hits.fetch_add(1, Ordering::Relaxed);
    }

    /// Total requests across routes (tests).
    #[cfg(test)]
    pub fn total(&self) -> u64 {
        self.requests.iter().map(|e| *e.value()).sum()
    }

    /// Prometheus text exposition.
    pub fn render(&self) -> String {
        let mut out =
            String::from("# HELP ares_gateway_requests_total Requests by route and status\n");
        out.push_str("# TYPE ares_gateway_requests_total counter\n");
        let mut rows: Vec<_> = self
            .requests
            .iter()
            .map(|e| (e.key().0.clone(), e.key().1, *e.value()))
            .collect();
        rows.sort();
        for (route, status, count) in rows {
            out.push_str(&format!(
                "ares_gateway_requests_total{{route=\"{route}\",status=\"{status}\"}} {count}\n"
            ));
        }
        out.push_str("# TYPE ares_gateway_request_duration_seconds_sum counter\n");
        out.push_str("# TYPE ares_gateway_request_duration_seconds_count counter\n");
        let mut lat: Vec<_> = self
            .latency
            .iter()
            .map(|e| (e.key().clone(), e.value().0, e.value().1))
            .collect();
        lat.sort();
        for (route, nanos, count) in lat {
            out.push_str(&format!(
                "ares_gateway_request_duration_seconds_sum{{route=\"{route}\"}} {}\n",
                nanos as f64 / 1e9
            ));
            out.push_str(&format!(
                "ares_gateway_request_duration_seconds_count{{route=\"{route}\"}} {count}\n"
            ));
        }
        out.push_str("# TYPE ares_gateway_cache_hits_total counter\n");
        out.push_str(&format!(
            "ares_gateway_cache_hits_total {}\n",
            self.cache_hits.load(Ordering::Relaxed)
        ));
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_counters_and_latency() {
        let m = Metrics::default();
        m.record("api", 200, Duration::from_millis(10));
        m.record("api", 200, Duration::from_millis(20));
        m.record("api", 500, Duration::from_millis(5));
        m.hit();
        assert_eq!(m.total(), 3);
        let text = m.render();
        assert!(text.contains("ares_gateway_requests_total{route=\"api\",status=\"200\"} 2"));
        assert!(text.contains("ares_gateway_requests_total{route=\"api\",status=\"500\"} 1"));
        assert!(text.contains("ares_gateway_request_duration_seconds_count{route=\"api\"} 3"));
        assert!(text.contains("ares_gateway_cache_hits_total 1"));
    }
}
