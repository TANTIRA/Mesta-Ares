//! Threat-intel provider lookups (VirusTotal, AbuseIPDB, Shodan, GreyNoise, urlscan).

use anyhow::{Context, Result};
use async_trait::async_trait;
use serde_json::Value;

use super::{arg_str, dry_run, Tool};

/// `shodan.host`, `greynoise.ip`, `urlscan.submit`.
pub(crate) struct ProviderLookup {
    pub(crate) name: &'static str,
    pub(crate) intel: connectors::intel::ThreatIntelClient,
}

#[async_trait]
impl Tool for ProviderLookup {
    fn name(&self) -> &'static str {
        self.name
    }
    async fn execute(&self, args: &Value) -> Result<Value> {
        let ioc = arg_str(args, &["ioc"]).context("missing ioc/value arg")?;
        match self.intel.lookup(self.name, ioc).await? {
            Some(raw) => Ok(raw),
            None => dry_run(args),
        }
    }
}
