//! IoC extraction and multi-provider threat-intel enrichment.
//!
//! [`extract_iocs`] pulls indicators out of free text; [`Enricher::enrich`]
//! fans one indicator out to VirusTotal / AbuseIPDB / Shodan / GreyNoise /
//! MISP in parallel with an 8s per-provider timeout, caches results in Redis
//! (`enr:{sha256(ioc)}`, 24h TTL), and tolerates individual provider failures.

use std::net::IpAddr;
use std::sync::LazyLock;
use std::time::Duration;

use connectors::{ConnectorConfig, CveRecord, MispClient, NvdClient};
use redis::AsyncCommands;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use tracing::{debug, warn};

const PROVIDER_TIMEOUT: Duration = Duration::from_secs(8);
const CACHE_TTL_SECS: u64 = 86_400;

static RE_IPV4: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\b(?:\d{1,3}\.){3}\d{1,3}\b").unwrap());
static RE_URL: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#"https?://[^\s"'<>)\]]+"#).unwrap());
static RE_DOMAIN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\b(?:[a-zA-Z0-9](?:[a-zA-Z0-9-]{0,61}[a-zA-Z0-9])?\.)+[a-zA-Z]{2,63}\b").unwrap()
});
static RE_MD5: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\b[0-9a-fA-F]{32}\b").unwrap());
static RE_SHA256: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\b[0-9a-fA-F]{64}\b").unwrap());
static RE_EMAIL: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\b[\w.+-]+@[\w-]+\.[\w.-]+\b").unwrap());
static RE_CVE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\bCVE-\d{4}-\d{4,7}\b").unwrap());

/// Indicators extracted from free text, deduplicated and sorted.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct IocSet {
    pub ips: Vec<String>,
    pub domains: Vec<String>,
    pub urls: Vec<String>,
    /// MD5 and SHA-256 hashes (lowercased).
    pub hashes_md5_sha256: Vec<String>,
    pub emails: Vec<String>,
    /// CVE identifiers (uppercased, e.g. `CVE-2024-3094`).
    pub cve_ids: Vec<String>,
}

fn sorted_unique(mut v: Vec<String>) -> Vec<String> {
    v.sort();
    v.dedup();
    v
}

/// True for RFC1918/loopback/link-local/multicast/unspecified/broadcast and
/// other non-public IPs that are never worth enriching.
fn is_uninteresting(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(a) => {
            a.is_private()
                || a.is_loopback()
                || a.is_link_local()
                || a.is_multicast()
                || a.is_unspecified()
                || a.is_broadcast()
                || a.is_documentation()
                || (a.octets()[0] == 100 && (a.octets()[1] & 0xC0) == 64) // CGNAT
        }
        IpAddr::V6(a) => a.is_loopback() || a.is_multicast() || a.is_unspecified(),
    }
}

/// Extract IoCs from free text. Private/unroutable IPs are stripped.
pub fn extract_iocs(text: &str) -> IocSet {
    let urls: Vec<String> = RE_URL
        .find_iter(text)
        .map(|m| {
            m.as_str()
                .trim_end_matches(['.', ',', ';', ')', ']', '\'', '"'])
                .to_string()
        })
        .collect();

    let emails: Vec<String> = RE_EMAIL
        .find_iter(text)
        .map(|m| m.as_str().to_string())
        .collect();

    let ips = RE_IPV4
        .find_iter(text)
        .filter_map(|m| m.as_str().parse::<IpAddr>().ok())
        .filter(|ip| !is_uninteresting(*ip))
        .map(|ip| ip.to_string())
        .collect();

    let hashes = RE_MD5
        .find_iter(text)
        .chain(RE_SHA256.find_iter(text))
        .map(|m| m.as_str().to_lowercase())
        .collect();

    // Drop domain tokens already covered by a URL match. Email domains stay:
    // they are indicators in their own right.
    let domains = RE_DOMAIN
        .find_iter(text)
        .map(|m| m.as_str().to_string())
        .filter(|d| !urls.iter().any(|u| u.contains(d.as_str())))
        .collect();

    let cve_ids = RE_CVE
        .find_iter(text)
        .map(|m| m.as_str().to_uppercase())
        .collect();

    IocSet {
        ips: sorted_unique(ips),
        domains: sorted_unique(domains),
        urls: sorted_unique(urls),
        hashes_md5_sha256: sorted_unique(hashes),
        emails: sorted_unique(emails),
        cve_ids: sorted_unique(cve_ids),
    }
}

/// Coarse IoC classification used to pick provider endpoints.
pub fn ioc_type(ioc: &str) -> &'static str {
    if ioc.parse::<IpAddr>().is_ok() {
        "ip"
    } else if RE_SHA256.is_match(ioc) || RE_MD5.is_match(ioc) {
        "hash"
    } else if ioc.starts_with("http://") || ioc.starts_with("https://") {
        "url"
    } else if RE_EMAIL.is_match(ioc) {
        "email"
    } else {
        "domain"
    }
}

/// One provider's verdict on an indicator.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnrichmentResult {
    pub ioc: String,
    pub ioc_type: String,
    pub source: String,
    pub malicious: bool,
    /// Provider-normalized confidence, 0–100.
    pub score: u8,
    pub raw: Value,
}

/// Weighted verdict across providers:
/// `vt_score*0.5 + abuseipdb_score*0.3 + (misp hit ? 20 : 0)`, capped at 100.
pub fn composite_score(results: &[EnrichmentResult]) -> u8 {
    let pick = |src: &str| {
        results
            .iter()
            .find(|r| r.source == src)
            .map(|r| r.score)
            .unwrap_or(0) as f32
    };
    let misp_bonus: f32 = if results.iter().any(|r| r.source == "misp" && r.malicious) {
        20.0
    } else {
        0.0
    };
    (pick("virustotal") * 0.5 + pick("abuseipdb") * 0.3 + misp_bonus).min(100.0) as u8
}

fn env(key: &str) -> Option<String> {
    std::env::var(key).ok().filter(|s| !s.is_empty())
}

/// Multi-provider IoC enrichment fan-out with Redis caching.
pub struct Enricher {
    redis: Option<redis::Client>,
    misp: Option<MispClient>,
    nvd: NvdClient,
    intel: connectors::intel::ThreatIntelClient,
}

impl Enricher {
    /// Build from env: `REDIS_URL`, `VIRUSTOTAL_API_KEY`, `SHODAN_API_KEY`,
    /// `ABUSEIPDB_API_KEY`, `GREYNOISE_API_KEY`, `MISP_URL`/`MISP_API_KEY`.
    pub fn new() -> Self {
        let redis = env("REDIS_URL").and_then(|u| redis::Client::open(u).ok());
        let cfg = ConnectorConfig::from_env();
        Self {
            redis,
            misp: cfg.build_misp_client(),
            nvd: cfg.build_nvd_client(),
            intel: connectors::intel::ThreatIntelClient::with_timeout(
                connectors::intel::IntelKeys::from_env(),
                PROVIDER_TIMEOUT,
            ),
        }
    }

    /// Enrich one indicator across all configured providers. Cache hit
    /// short-circuits; provider errors are logged and skipped.
    pub async fn enrich(&self, ioc: &str) -> Vec<EnrichmentResult> {
        let key = cache_key(ioc);
        if let Some(hit) = self.cache_get(&key).await {
            debug!(ioc, "enrichment cache hit");
            return hit;
        }

        let kind = ioc_type(ioc);
        let futs: Vec<futures::future::BoxFuture<'_, Option<EnrichmentResult>>> = vec![
            Box::pin(self.virustotal(ioc, kind)),
            Box::pin(self.abuseipdb(ioc, kind)),
            Box::pin(self.shodan(ioc, kind)),
            Box::pin(self.greynoise(ioc, kind)),
            Box::pin(self.misp_lookup(ioc, kind)),
        ];
        let results: Vec<EnrichmentResult> = futures::future::join_all(futs)
            .await
            .into_iter()
            .flatten()
            .collect();

        self.cache_set(&key, &results).await;
        results
    }

    async fn cache_get(&self, key: &str) -> Option<Vec<EnrichmentResult>> {
        let client = self.redis.as_ref()?;
        let mut conn = client.get_multiplexed_async_connection().await.ok()?;
        let raw: Option<String> = conn.get(key).await.ok()?;
        serde_json::from_str(&raw?).ok()
    }

    async fn cache_set(&self, key: &str, results: &[EnrichmentResult]) {
        let Some(client) = self.redis.as_ref() else {
            return;
        };
        let Ok(payload) = serde_json::to_string(results) else {
            return;
        };
        match client.get_multiplexed_async_connection().await {
            Ok(mut conn) => {
                let r: redis::RedisResult<()> = conn.set_ex(key, payload, CACHE_TTL_SECS).await;
                if let Err(e) = r {
                    warn!(error = %e, "enrichment cache set failed");
                }
            }
            Err(e) => warn!(error = %e, "enrichment cache connect failed"),
        }
    }

    async fn virustotal(&self, ioc: &str, kind: &str) -> Option<EnrichmentResult> {
        if !matches!(kind, "ip" | "domain" | "hash" | "url") {
            return None;
        }
        let raw = self
            .intel
            .lookup("virustotal.lookup", ioc)
            .await
            .map_err(|e| warn!(error = %e, "virustotal request failed"))
            .ok()??;
        let stats = &raw["data"]["attributes"]["last_analysis_stats"];
        let malicious = stats["malicious"].as_u64().unwrap_or(0);
        let total: u64 = ["malicious", "suspicious", "harmless", "undetected"]
            .iter()
            .map(|k| stats[k].as_u64().unwrap_or(0))
            .sum();
        let score = if total > 0 {
            (malicious
                .checked_mul(100)
                .and_then(|n| n.checked_div(total))
                .unwrap_or(0))
            .min(100) as u8
        } else {
            0
        };
        Some(EnrichmentResult {
            ioc: ioc.to_string(),
            ioc_type: kind.to_string(),
            source: "virustotal".to_string(),
            malicious: malicious > 0,
            score,
            raw,
        })
    }

    async fn abuseipdb(&self, ioc: &str, kind: &str) -> Option<EnrichmentResult> {
        if kind != "ip" {
            return None;
        }
        let raw = self
            .intel
            .lookup("abuseipdb.check", ioc)
            .await
            .map_err(|e| warn!(error = %e, "abuseipdb request failed"))
            .ok()??;
        let score = raw["data"]["abuseConfidenceScore"]
            .as_u64()
            .unwrap_or(0)
            .min(100) as u8;
        Some(EnrichmentResult {
            ioc: ioc.to_string(),
            ioc_type: kind.to_string(),
            source: "abuseipdb".to_string(),
            malicious: score >= 25,
            score,
            raw,
        })
    }

    async fn shodan(&self, ioc: &str, kind: &str) -> Option<EnrichmentResult> {
        if kind != "ip" {
            return None;
        }
        let raw = self
            .intel
            .lookup("shodan.host", ioc)
            .await
            .map_err(|e| warn!(error = %e, "shodan request failed"))
            .ok()??;
        let vulns = raw["vulns"].as_array().map(|v| v.len()).unwrap_or(0);
        let score = (30 + 15 * vulns as u64).min(100) as u8;
        Some(EnrichmentResult {
            ioc: ioc.to_string(),
            ioc_type: kind.to_string(),
            source: "shodan".to_string(),
            malicious: vulns > 0,
            score,
            raw,
        })
    }

    async fn greynoise(&self, ioc: &str, kind: &str) -> Option<EnrichmentResult> {
        if kind != "ip" {
            return None;
        }
        let raw = self
            .intel
            .lookup("greynoise.ip", ioc)
            .await
            .map_err(|e| warn!(error = %e, "greynoise request failed"))
            .ok()??;
        let classification = raw["classification"].as_str().unwrap_or("");
        let (malicious, score) = match classification {
            "malicious" => (true, 90),
            "benign" => (false, 10),
            _ => (false, 30),
        };
        Some(EnrichmentResult {
            ioc: ioc.to_string(),
            ioc_type: kind.to_string(),
            source: "greynoise".to_string(),
            malicious,
            score,
            raw,
        })
    }

    /// Look up one CVE in NVD (API 2.0), Redis-cached as
    /// `nvd:cve:{CVE-ID}` for 24h. Misses are negative-cached as `null` for
    /// 1h so rejected/reserved IDs don't hammer the rate limit. NVD errors
    /// return `None` — never fatal to the caller.
    pub async fn enrich_cve(&self, cve_id: &str) -> anyhow::Result<Option<CveRecord>> {
        let cve_id = cve_id.to_uppercase();
        let key = format!("nvd:cve:{cve_id}");
        if let Some(client) = &self.redis {
            if let Ok(mut conn) = client.get_multiplexed_async_connection().await {
                if let Ok(Some(raw)) = conn.get::<_, Option<String>>(&key).await {
                    if raw == "null" {
                        return Ok(None);
                    }
                    if let Ok(rec) = serde_json::from_str::<CveRecord>(&raw) {
                        debug!(cve = %cve_id, "nvd cache hit");
                        return Ok(Some(rec));
                    }
                }
            }
        }
        match self.nvd.get_cve(&cve_id).await {
            Ok(Some(rec)) => {
                if let Some(client) = &self.redis {
                    if let Ok(payload) = serde_json::to_string(&rec) {
                        if let Ok(mut conn) = client.get_multiplexed_async_connection().await {
                            let _: redis::RedisResult<()> =
                                conn.set_ex(&key, payload, CACHE_TTL_SECS).await;
                        }
                    }
                }
                Ok(Some(rec))
            }
            Ok(None) => {
                if let Some(client) = &self.redis {
                    if let Ok(mut conn) = client.get_multiplexed_async_connection().await {
                        let _: redis::RedisResult<()> = conn.set_ex(&key, "null", 3600).await;
                    }
                }
                Ok(None)
            }
            Err(e) => {
                warn!(error = %e, cve = %cve_id, "nvd lookup failed");
                Err(e)
            }
        }
    }

    async fn misp_lookup(&self, ioc: &str, kind: &str) -> Option<EnrichmentResult> {
        let client = self.misp.as_ref()?;
        match client.search_attributes(ioc, 50).await {
            Ok(attributes) => {
                let hit = !attributes.is_empty();
                // A hit that only matches warninglists (known-benign ranges,
                // CDN addresses, ...) is not malicious.
                let warninglisted = hit
                    && client
                        .check_warninglists(&[ioc.to_string()])
                        .await
                        .map(|m| m.iter().any(|v| v == ioc))
                        .unwrap_or(false);
                let malicious = hit && !warninglisted;
                Some(EnrichmentResult {
                    ioc: ioc.to_string(),
                    ioc_type: kind.to_string(),
                    source: "misp".to_string(),
                    malicious,
                    score: if malicious { 80 } else { 0 },
                    raw: serde_json::json!({
                        "attributes": attributes,
                        "warninglist_match": warninglisted,
                    }),
                })
            }
            Err(e) => {
                warn!(error = %e, "misp lookup failed");
                None
            }
        }
    }
}

impl Default for Enricher {
    fn default() -> Self {
        Self::new()
    }
}

/// `enr:{sha256(ioc)}` — Redis cache key for an indicator.
fn cache_key(ioc: &str) -> String {
    format!("enr:{:x}", Sha256::digest(ioc.as_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_and_filters_iocs() {
        let text = "hit https://evil.example.com/x?a=1 from 8.8.8.8 and 10.0.0.5, \
                    md5 d41d8cd98f00b204e9800998ecf8427e mail to soc@corp.example \
                    sha256 7d793037a0760186574b0282f2f435e7ae99771ded99311613c0d09860ad7c35";
        let s = extract_iocs(text);
        assert_eq!(s.ips, vec!["8.8.8.8"]);
        assert_eq!(s.urls, vec!["https://evil.example.com/x?a=1"]);
        assert!(s
            .hashes_md5_sha256
            .contains(&"d41d8cd98f00b204e9800998ecf8427e".into()));
        assert!(s
            .hashes_md5_sha256
            .contains(&"7d793037a0760186574b0282f2f435e7ae99771ded99311613c0d09860ad7c35".into()));
        assert_eq!(s.emails, vec!["soc@corp.example"]);
        // corp.example stays; evil.example.com is inside the URL.
        assert!(s.domains.contains(&"corp.example".to_string()));
        assert!(!s.domains.contains(&"evil.example.com".to_string()));
    }

    #[test]
    fn composite_score_weights() {
        let r = |source: &str, malicious: bool, score: u8| EnrichmentResult {
            ioc: "x".into(),
            ioc_type: "ip".into(),
            source: source.into(),
            malicious,
            score,
            raw: Value::Null,
        };
        assert_eq!(composite_score(&[]), 0);
        assert_eq!(composite_score(&[r("virustotal", true, 80)]), 40);
        assert_eq!(
            composite_score(&[r("virustotal", true, 80), r("abuseipdb", true, 100)]),
            70
        );
        assert_eq!(
            composite_score(&[
                r("virustotal", true, 100),
                r("abuseipdb", true, 100),
                r("misp", true, 80),
            ]),
            100
        );
    }
}
