//! Direct-API threat-intel provider lookups.
//!
//! One HTTP implementation of the VirusTotal / AbuseIPDB / Shodan / GreyNoise
//! / urlscan calls, shared by [`crate`]-based enrichment and the tool-server
//! executors. Callers own normalization and scoring.

use anyhow::{Context, Result};
use reqwest::Client;
use serde_json::{json, Value};

/// API keys for the direct-API providers; `None` means unconfigured.
#[derive(Debug, Clone, Default)]
pub struct IntelKeys {
    /// VirusTotal API key (`x-apikey`).
    pub virustotal: Option<String>,
    /// AbuseIPDB API key (`Key` header).
    pub abuseipdb: Option<String>,
    /// Shodan API key (query parameter).
    pub shodan: Option<String>,
    /// GreyNoise API key (`key` header).
    pub greynoise: Option<String>,
    /// urlscan.io API key (`API-Key` header).
    pub urlscan: Option<String>,
}

impl IntelKeys {
    /// Reads `VIRUSTOTAL_API_KEY`, `ABUSEIPDB_API_KEY`, `SHODAN_API_KEY`,
    /// `GREYNOISE_API_KEY` and `URLSCAN_API_KEY`.
    pub fn from_env() -> Self {
        Self {
            virustotal: env("VIRUSTOTAL_API_KEY"),
            abuseipdb: env("ABUSEIPDB_API_KEY"),
            shodan: env("SHODAN_API_KEY"),
            greynoise: env("GREYNOISE_API_KEY"),
            urlscan: env("URLSCAN_API_KEY"),
        }
    }
}

/// Reads `key` from the environment, treating empty values as unset.
pub fn env(key: &str) -> Option<String> {
    std::env::var(key).ok().filter(|s| !s.is_empty())
}

/// base64url without padding (VirusTotal URL ids).
pub fn b64url(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        out.push(ALPHABET[(n >> 18) as usize & 63] as char);
        out.push(ALPHABET[(n >> 12) as usize & 63] as char);
        if chunk.len() > 1 {
            out.push(ALPHABET[(n >> 6) as usize & 63] as char);
        }
        if chunk.len() > 2 {
            out.push(ALPHABET[n as usize & 63] as char);
        }
    }
    out
}

/// Client for the direct-API threat-intel providers.
#[derive(Clone)]
pub struct ThreatIntelClient {
    http: Client,
    keys: IntelKeys,
}

impl ThreatIntelClient {
    /// Builds a client from explicit keys.
    pub fn new(keys: IntelKeys) -> Self {
        Self {
            http: Client::new(),
            keys,
        }
    }

    /// Builds a client with keys read from the environment.
    pub fn from_env() -> Self {
        Self::new(IntelKeys::from_env())
    }

    /// Builds a client whose requests time out after `timeout`.
    pub fn with_timeout(keys: IntelKeys, timeout: std::time::Duration) -> Self {
        Self {
            http: Client::builder()
                .timeout(timeout)
                .build()
                .unwrap_or_else(|_| Client::new()),
            keys,
        }
    }

    /// Runs the `provider` lookup for `ioc`.
    ///
    /// Returns `Ok(None)` when the provider's key is unset, so callers can
    /// fall back to a dry-run response.
    pub async fn lookup(&self, provider: &str, ioc: &str) -> Result<Option<Value>> {
        match provider {
            "virustotal.lookup" => {
                let Some(key) = self.keys.virustotal.as_deref() else {
                    return Ok(None);
                };
                Ok(Some(
                    self.http
                        .get(format!(
                            "https://www.virustotal.com/api/v3/{}",
                            virustotal_path(ioc)
                        ))
                        .header("x-apikey", key)
                        .send()
                        .await
                        .context("virustotal request")?
                        .error_for_status()
                        .context("virustotal HTTP error")?
                        .json::<Value>()
                        .await?,
                ))
            }
            "abuseipdb.check" => {
                let Some(key) = self.keys.abuseipdb.as_deref() else {
                    return Ok(None);
                };
                Ok(Some(
                    self.http
                        .get("https://api.abuseipdb.com/api/v2/check")
                        .header("Key", key)
                        .header("Accept", "application/json")
                        .query(&[("ipAddress", ioc), ("maxAgeInDays", "90")])
                        .send()
                        .await
                        .context("abuseipdb request")?
                        .error_for_status()
                        .context("abuseipdb HTTP error")?
                        .json::<Value>()
                        .await?,
                ))
            }
            "shodan.host" => {
                let Some(key) = self.keys.shodan.as_deref() else {
                    return Ok(None);
                };
                Ok(Some(
                    self.http
                        .get(format!("https://api.shodan.io/shodan/host/{ioc}"))
                        .query(&[("key", key)])
                        .send()
                        .await
                        .context("shodan request")?
                        .error_for_status()
                        .context("shodan HTTP error")?
                        .json::<Value>()
                        .await?,
                ))
            }
            "greynoise.ip" => {
                let Some(key) = self.keys.greynoise.as_deref() else {
                    return Ok(None);
                };
                Ok(Some(
                    self.http
                        .get(format!("https://api.greynoise.io/v3/community/{ioc}"))
                        .header("key", key)
                        .send()
                        .await
                        .context("greynoise request")?
                        .error_for_status()
                        .context("greynoise HTTP error")?
                        .json::<Value>()
                        .await?,
                ))
            }
            "urlscan.submit" => {
                let Some(key) = self.keys.urlscan.as_deref() else {
                    return Ok(None);
                };
                Ok(Some(
                    self.http
                        .post("https://urlscan.io/api/v1/scan/")
                        .header("API-Key", key)
                        .json(&json!({ "url": ioc, "visibility": "private" }))
                        .send()
                        .await
                        .context("urlscan request")?
                        .error_for_status()
                        .context("urlscan HTTP error")?
                        .json::<Value>()
                        .await?,
                ))
            }
            other => anyhow::bail!("unknown threat-intel provider `{other}`"),
        }
    }
}

/// VirusTotal v3 path segment for an indicator, classified by its shape.
fn virustotal_path(ioc: &str) -> String {
    if ioc.parse::<std::net::IpAddr>().is_ok() {
        format!("ip_addresses/{ioc}")
    } else if ioc.starts_with("http://") || ioc.starts_with("https://") {
        format!("urls/{}", b64url(ioc.as_bytes()))
    } else if ioc.len() == 64 || ioc.len() == 32 {
        format!("files/{ioc}")
    } else {
        format!("domains/{ioc}")
    }
}

#[cfg(test)]
mod tests {
    use super::b64url;

    #[test]
    fn b64url_matches_known_encoding() {
        assert_eq!(b64url(b"hello"), "aGVsbG8");
        assert_eq!(b64url(b"http://a.io/x"), "aHR0cDovL2EuaW8veA");
        assert_eq!(b64url(b""), "");
    }
}
