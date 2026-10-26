//! Tool executors and the startup registry.
//!
//! Every entry in `policies/tools.yml` maps to one [`Tool`] implementation.
//! Executors holding a real backend make the connector call when the required
//! env vars exist; otherwise they return `{ "dry_run": true, args }` so the
//! pipeline stays exercisable without credentials.
//!
//! Layout: one module per backend ([`opensearch`], [`thehive`], [`cortex`],
//! [`misp`], [`wazuh`], [`shuffle`], [`providers`], [`identity`], [`nvd`],
//! [`openshell`]); this file holds the [`Tool`] trait, the shared argument
//! helpers and the registry.

mod cortex;
mod identity;
mod misp;
mod nvd;
mod opensearch;
mod openshell;
mod providers;
mod shuffle;
mod thehive;
mod wazuh;

use std::collections::HashMap;
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use self::cortex::CortexAnalyzer;
use self::identity::EntraUser;
use self::identity::SlackNotify;
use self::misp::MispAttribute;
use self::misp::MispAttributeSearch;
use self::misp::MispEventView;
use self::misp::MispSearch;
use self::misp::MispSighting;
use self::misp::MispWarninglist;
use self::nvd::CvssScoreTool;
use self::nvd::DryRun;
use self::nvd::NvdTool;
use self::opensearch::OpensearchQuery;
use self::providers::ProviderLookup;
use self::shuffle::ShuffleWorkflow;
use self::thehive::ThehiveCase;
use self::thehive::ThehiveCaseUpdate;
use self::thehive::ThehiveObservable;
use self::wazuh::WazuhActiveResponse;
use self::wazuh::WazuhAgents;
use anyhow::Result;
use async_trait::async_trait;
use connectors::ConnectorConfig;
use serde_json::{json, Value};

/// A dispatchable tool executor.
#[async_trait]
pub trait Tool: Send + Sync {
    /// Allowlist name this executor serves.
    fn name(&self) -> &'static str;
    /// Execute the (already policy-approved) call.
    async fn execute(&self, args: &Value) -> Result<Value>;
}

pub(crate) fn env(key: &str) -> Option<String> {
    std::env::var(key).ok().filter(|s| !s.is_empty())
}

pub(crate) fn dry_run(args: &Value) -> Result<Value> {
    Ok(json!({ "dry_run": true, "args": args }))
}

pub(crate) fn arg_str<'a>(args: &'a Value, keys: &[&str]) -> Option<&'a str> {
    keys.iter().find_map(|k| args.get(k)?.as_str())
}

/// Shared HTTP client for executors that hit provider APIs directly.
pub(crate) fn http() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new())
    })
}

// ---------------------------------------------------------------------------
// Registry
// ---------------------------------------------------------------------------

/// Build the tool registry covering every name in `policies/tools.yml`.
pub fn build_registry(cfg: &ConnectorConfig) -> HashMap<String, Arc<dyn Tool>> {
    let mut reg: HashMap<String, Arc<dyn Tool>> = HashMap::new();
    let mut put = |t: Arc<dyn Tool>| {
        reg.insert(t.name().to_string(), t);
    };

    // One client (and connection pool) per backend, cloned into executors.
    let os = cfg.build_opensearch_client();
    let hive = cfg.build_thehive_client();
    let cortex = cfg.build_cortex_client();
    let misp = cfg.build_misp_client();
    let shuffle = cfg.build_shuffle_client();
    let wazuh = cfg.build_wazuh_client();

    // Read-only / enrichment
    put(Arc::new(OpensearchQuery(os)));
    put(Arc::new(ThehiveCase(hive.clone())));
    put(Arc::new(ThehiveCaseUpdate(hive.clone())));
    put(Arc::new(ThehiveObservable(hive)));
    put(Arc::new(CortexAnalyzer(cortex)));
    put(Arc::new(MispSearch(misp.clone())));
    put(Arc::new(MispAttribute {
        name: "misp.attribute.add",
        client: misp.clone(),
    }));
    put(Arc::new(MispAttribute {
        name: "misp.ioc.block",
        client: misp.clone(),
    }));
    put(Arc::new(MispAttributeSearch(misp.clone())));
    put(Arc::new(MispEventView(misp.clone())));
    put(Arc::new(MispWarninglist(misp.clone())));
    put(Arc::new(MispSighting(misp)));
    let intel = connectors::intel::ThreatIntelClient::from_env();
    for name in [
        "virustotal.lookup",
        "abuseipdb.check",
        "shodan.host",
        "greynoise.ip",
        "urlscan.submit",
    ] {
        put(Arc::new(ProviderLookup {
            name,
            intel: intel.clone(),
        }));
    }
    put(Arc::new(DryRun {
        name: "osquery.fleet_query",
    }));
    put(Arc::new(WazuhAgents(wazuh.clone())));

    // NVD / CVSS
    let nvd = Arc::new(cfg.build_nvd_client());
    for name in ["nvd.lookup_cve", "nvd.cve_history", "nvd.recent_cves"] {
        put(Arc::new(NvdTool {
            name,
            client: nvd.clone(),
        }));
    }
    put(Arc::new(CvssScoreTool));

    // Response actions
    put(Arc::new(ShuffleWorkflow {
        name: "blocklist.ip.add",
        env_key: "SHUFFLE_BLOCKLIST_WORKFLOW_ID",
        client: shuffle.clone(),
    }));
    put(Arc::new(ShuffleWorkflow {
        name: "blocklist.domain.add",
        env_key: "SHUFFLE_DOMAIN_WORKFLOW_ID",
        client: shuffle.clone(),
    }));
    put(Arc::new(ShuffleWorkflow {
        name: "firewall.rule.add",
        env_key: "SHUFFLE_FW_WORKFLOW_ID",
        client: shuffle.clone(),
    }));
    put(Arc::new(WazuhActiveResponse {
        name: "wazuh.active_response.isolate_host",
        command: "isolate-host",
        client: wazuh.clone(),
    }));
    put(Arc::new(WazuhActiveResponse {
        name: "wazuh.active_response.kill_process",
        command: "kill-process",
        client: wazuh.clone(),
    }));
    put(Arc::new(WazuhActiveResponse {
        name: "wazuh.active_response.unisolate_host",
        command: "unisolate-host",
        client: wazuh.clone(),
    }));
    // Compensating blocklist/firewall actions run the matching Shuffle
    // removal workflow; an unset workflow id falls back to dry-run.
    for (name, env_key) in [
        ("blocklist.ip.remove", "SHUFFLE_UNBLOCK_IP_WORKFLOW_ID"),
        (
            "blocklist.domain.remove",
            "SHUFFLE_UNBLOCK_DOMAIN_WORKFLOW_ID",
        ),
        ("firewall.rule.remove", "SHUFFLE_FW_REMOVE_WORKFLOW_ID"),
    ] {
        put(Arc::new(ShuffleWorkflow {
            name,
            env_key,
            client: shuffle.clone(),
        }));
    }
    put(Arc::new(EntraUser {
        name: "entra.user.revoke_sessions",
    }));
    put(Arc::new(EntraUser {
        name: "entra.user.disable",
    }));
    put(Arc::new(EntraUser {
        name: "entra.user.enable",
    }));
    put(Arc::new(DryRun {
        name: "ad.user.disable",
    }));
    put(Arc::new(DryRun {
        name: "ad.user.enable",
    }));
    put(Arc::new(DryRun {
        name: "o365.email.quarantine",
    }));
    put(Arc::new(DryRun {
        name: "o365.email.release",
    }));
    put(Arc::new(DryRun {
        name: "misp.ioc.unblock",
    }));

    // Detection-as-code + notifications
    put(Arc::new(DryRun {
        name: "detection.sigma.deploy",
    }));
    put(Arc::new(SlackNotify));

    // Dynamic command execution — always through the OpenShell sandbox (see
    // openshell.rs). `exec.bas.run` is registered ahead of its allowlist
    // entry so BAS local-exec lands in the sandbox the day policy enables it.
    put(Arc::new(openshell::OpenShellExec {
        name: "exec.sandbox",
    }));
    put(Arc::new(openshell::OpenShellExec {
        name: "exec.bas.run",
    }));

    // Explicitly denied by policy; registered so the registry covers the
    // whole allowlist file. Dispatch is never reached for these.
    for name in ["host.shutdown", "user.delete", "siem.index.delete"] {
        put(Arc::new(DryRun { name }));
    }

    reg
}

/// Names in the allowlist with no registered executor (startup sanity check).
pub fn missing_executors(
    registry: &HashMap<String, Arc<dyn Tool>>,
    allowlist: &[String],
) -> Vec<String> {
    allowlist
        .iter()
        .filter(|t| !registry.contains_key(*t))
        .cloned()
        .collect()
}

#[cfg(test)]
mod registry_tests {
    use super::*;

    /// Every tool named in `policies/tools.yml` must have an executor, or
    /// startup aborts (`missing_executors` in main).
    #[test]
    fn registry_covers_policy_tools() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../policies");
        let engine = policy_engine::PolicyEngine::load(&dir).expect("load policies");
        let reg = build_registry(&ConnectorConfig::from_env());
        assert!(
            missing_executors(&reg, &engine.tool_names()).is_empty(),
            "policy tools without executors: {:?}",
            missing_executors(&reg, &engine.tool_names())
        );
    }
}
