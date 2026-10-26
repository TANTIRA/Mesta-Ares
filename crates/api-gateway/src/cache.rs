//! TTL cache for cacheable GET responses.
//!
//! Only `200` responses under a configured prefix are stored, and only when
//! the body fits the size cap. Entries expire after `GATEWAY_CACHE_TTL_SECONDS`
//! (0 disables caching entirely).

use std::time::{Duration, Instant};

use dashmap::DashMap;

/// Largest body the cache will hold.
const MAX_BODY: usize = 1024 * 1024;

/// One cached response.
#[derive(Clone)]
pub struct Cached {
    /// Original status (always 200 today).
    pub status: u16,
    /// `content-type` of the stored body.
    pub content_type: Option<String>,
    /// Response body.
    pub body: Vec<u8>,
}

struct Entry {
    stored: Instant,
    cached: Cached,
}

/// Prefix-scoped TTL cache.
pub struct ResponseCache {
    ttl: Duration,
    prefixes: Vec<String>,
    entries: DashMap<String, Entry>,
}

impl ResponseCache {
    /// Builds a cache for `prefixes` (path prefixes such as `/api/vulns`).
    pub fn new(ttl: Duration, prefixes: Vec<String>) -> Self {
        Self {
            ttl,
            prefixes,
            entries: DashMap::new(),
        }
    }

    /// `true` when this request may be cached and served from cache.
    pub fn applies(&self, method: &str, path: &str) -> bool {
        !self.ttl.is_zero() && method == "GET" && self.prefixes.iter().any(|p| path.starts_with(p))
    }

    /// Looks up a live entry.
    pub fn get(&self, key: &str) -> Option<Cached> {
        let entry = self.entries.get(key)?;
        if entry.stored.elapsed() > self.ttl {
            drop(entry);
            self.entries.remove(key);
            return None;
        }
        Some(entry.cached.clone())
    }

    /// Stores a response body when it fits the cap.
    pub fn put(&self, key: &str, status: u16, content_type: Option<String>, body: &[u8]) {
        if status != 200 || body.len() > MAX_BODY {
            return;
        }
        self.entries.insert(
            key.to_string(),
            Entry {
                stored: Instant::now(),
                cached: Cached {
                    status,
                    content_type,
                    body: body.to_vec(),
                },
            },
        );
    }

    /// Number of live entries (tests).
    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.entries.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn applies_only_to_get_under_prefix() {
        let c = ResponseCache::new(Duration::from_secs(30), vec!["/api/vulns".into()]);
        assert!(c.applies("GET", "/api/vulns"));
        assert!(c.applies("GET", "/api/vulns/CVE-2024-3094"));
        assert!(!c.applies("GET", "/api/alerts"));
        assert!(!c.applies("POST", "/api/vulns"));
        let off = ResponseCache::new(Duration::ZERO, vec!["/api/vulns".into()]);
        assert!(!off.applies("GET", "/api/vulns"));
    }

    #[test]
    fn stores_and_expires() {
        let c = ResponseCache::new(Duration::from_millis(20), vec!["/x".into()]);
        c.put("k", 200, Some("application/json".into()), b"{}");
        assert!(c.get("k").is_some());
        std::thread::sleep(Duration::from_millis(40));
        assert!(c.get("k").is_none());
        assert_eq!(c.len(), 0, "expired entry is evicted on read");
    }

    #[test]
    fn skips_errors_and_oversized_bodies() {
        let c = ResponseCache::new(Duration::from_secs(30), vec!["/x".into()]);
        c.put("e", 500, None, b"oops");
        assert!(c.get("e").is_none());
        let big = vec![0u8; MAX_BODY + 1];
        c.put("big", 200, None, &big);
        assert!(c.get("big").is_none());
    }
}
