//! Detection-as-code pipeline: draft → validate → backtest → promote → deploy.

use std::path::{Path, PathBuf};
use std::process::Command;

use agent_protocol::tool::ToolCall;
use anyhow::Context;
use connectors::OpensearchClient;
use llm_client::{CompletionRequest, Message, ModelRouter, ModelTier, Role, strong_model};
use serde_json::{json, Value};
use tracing::{info, warn};
use uuid::Uuid;

/// Directory for rules awaiting promotion.
const PENDING_DIR: &str = "detection-content/pending";
/// Directory for active (deployed) rules.
const ACTIVE_DIR: &str = "detection-content/active";
/// Index holding alert/doc data to backtest against.
const BACKTEST_INDEX: &str = "wazuh-alerts-*";
/// Backtest lookback window.
const BACKTEST_LOOKBACK: &str = "now-7d";

/// Strict-JSON/yaml prompt for Sigma drafting.
const DRAFT_PROMPT: &str = r#"You are a detection engineer. Given the context (uncovered alerts, new
threat intel, BAS failures), draft ONE Sigma detection rule in YAML. Return
ONLY the YAML document — no prose, no markdown fences. Required top-level
keys: title, id (uuid), status: experimental, description, logsource
(product: wazuh), detection (with a `condition`), tags in attack.t#### form,
level."#;

/// Drafts a Sigma rule with the Strong model and returns the YAML text.
pub async fn draft_sigma(router: &ModelRouter, context: &Value) -> anyhow::Result<String> {
    let req = CompletionRequest {
        model: strong_model(),
        system: Some(DRAFT_PROMPT.to_string()),
        messages: vec![Message {
            role: Role::User,
            content: serde_json::to_string(context)?,
        }],
        max_tokens: 2048,
        temperature: Some(0.0),
    };
    let resp = router.complete(ModelTier::Strong, &req).await?;
    let text = resp
        .text
        .trim()
        .trim_start_matches("```yaml")
        .trim_start_matches("```yml")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim()
        .to_string();
    Ok(text)
}

/// Validates `yaml_text` — `sigma check` when the binary exists, otherwise a
/// YAML parse plus a minimal required-key check.
///
/// Returns the parsed YAML value for downstream steps.
pub fn validate_sigma(yaml_text: &str) -> anyhow::Result<Value> {
    // Ensure the text parses as YAML before invoking external tooling.
    let doc: Value = serde_yaml_ng::from_str(yaml_text).context("sigma draft is not valid YAML")?;

    if which("sigma").is_some() {
        // `sigma check` needs a file on disk; use a temp file.
        let tmp = std::env::temp_dir().join(format!("ares-{}.yml", Uuid::new_v4()));
        std::fs::write(&tmp, yaml_text)?;
        let status = Command::new("sigma").arg("check").arg(&tmp).status();
        let _ = std::fs::remove_file(&tmp);
        match status {
            Ok(s) if s.success() => info!("sigma check passed"),
            Ok(s) => anyhow::bail!("sigma check failed with {s}"),
            Err(e) => {
                warn!(error = %e, "sigma binary found but check failed to run; YAML validation only");
            }
        }
    } else {
        // Simplification: without the sigma toolchain we enforce only the
        // required Sigma keys rather than the full spec.
        for key in ["title", "detection", "logsource"] {
            if doc.get(key).is_none() {
                anyhow::bail!("sigma draft missing required key `{key}`");
            }
        }
        info!("sigma binary absent; YAML validation only");
    }
    Ok(doc)
}

/// Writes the draft to `detection-content/pending/{slug}.yml`.
pub fn write_pending(doc: &Value, yaml_text: &str) -> anyhow::Result<PathBuf> {
    std::fs::create_dir_all(PENDING_DIR)?;
    let title = doc
        .get("title")
        .and_then(Value::as_str)
        .unwrap_or("untitled-rule");
    let slug = slugify(title);
    let path = PathBuf::from(PENDING_DIR).join(format!("{slug}.yml"));
    std::fs::write(&path, yaml_text)?;
    Ok(path)
}

/// Converts a Sigma YAML rule to a Lucene query string for backtesting.
///
/// Uses `sigma convert -t lucene` when available; otherwise performs a naive
/// term mapping of `field: value` pairs under `detection.selection`.
///
/// SIMPLIFICATION: the naive backend only handles `key: value` scalars in a
/// single `selection` block — lists, modifiers (`contains`, `endswith`), and
/// multi-condition logic are collapsed to an AND of the mapped terms.
pub fn to_lucene(path: &Path, doc: &Value) -> anyhow::Result<String> {
    if which("sigma").is_some() {
        let out = Command::new("sigma")
            .args(["convert", "-t", "lucene"])
            .arg(path)
            .output();
        if let Ok(o) = out {
            if o.status.success() {
                let q = String::from_utf8_lossy(&o.stdout).trim().to_string();
                if !q.is_empty() {
                    return Ok(q);
                }
            }
        }
        warn!("sigma convert failed; falling back to naive term mapping");
    }
    naive_lucene(doc)
}

/// Naive `field:value AND field:value` Lucene mapping for `detection.selection`.
fn naive_lucene(doc: &Value) -> anyhow::Result<String> {
    let selection = doc
        .get("detection")
        .and_then(|d| d.get("selection"))
        .and_then(Value::as_object)
        .context("no detection.selection to map")?;
    let terms: Vec<String> = selection
        .iter()
        .filter_map(|(k, v)| v.as_str().map(|s| format!("{k}:{s}")))
        .collect();
    if terms.is_empty() {
        anyhow::bail!("detection.selection has no scalar terms to map");
    }
    Ok(terms.join(" AND "))
}

/// Runs the backtest against the last 7 days and returns the hit count
/// (treated as potential false positives).
pub async fn backtest(os: &OpensearchClient, lucene: &str) -> anyhow::Result<u64> {
    let query = json!({
        "size": 0,
        "track_total_hits": true,
        "query": {
            "bool": {
                "must": [{"query_string": {"query": lucene}}],
                "filter": [{"range": {"@timestamp": {"gte": BACKTEST_LOOKBACK}}}]
            }
        }
    });
    let resp = os.query(BACKTEST_INDEX, "", query).await?;
    let hits = resp
        .get("hits")
        .and_then(|h| h.get("total"))
        .and_then(|t| t.get("value").or(Some(t)).and_then(Value::as_u64))
        .unwrap_or(0);
    Ok(hits)
}

/// Moves a pending rule to `detection-content/active/` and commits it via git.
pub fn promote_and_commit(pending: &Path) -> anyhow::Result<PathBuf> {
    std::fs::create_dir_all(ACTIVE_DIR)?;
    let active = PathBuf::from(ACTIVE_DIR).join(
        pending
            .file_name()
            .ok_or_else(|| anyhow::anyhow!("pending path has no file name"))?,
    );
    std::fs::rename(pending, &active)?;
    // Git commit best-effort; a dirty/uninitialized repo only warns.
    let status = Command::new("git")
        .args(["add", &active.to_string_lossy()])
        .status()
        .and_then(|_| {
            Command::new("git")
                .args([
                    "commit",
                    "-m",
                    &format!("detection: promote {}", active.display()),
                ])
                .status()
        });
    match status {
        Ok(s) if s.success() => info!(rule = %active.display(), "rule committed"),
        Ok(s) => warn!(status = %s, "git commit returned non-zero"),
        Err(e) => warn!(error = %e, "git not available; skipping commit"),
    }
    Ok(active)
}

/// Deploys an OpenSearch alerting monitor for the rule.
///
/// Sent as a raw POST to `{OPENSEARCH_URL}/_plugins/_alerting/monitors`
/// because [`OpensearchClient::query`] only covers searches.
pub async fn deploy_monitor(
    http: &reqwest::Client,
    name: &str,
    lucene: &str,
) -> anyhow::Result<()> {
    let base =
        std::env::var("OPENSEARCH_URL").unwrap_or_else(|_| "http://localhost:9200".to_string());
    let auth = match (
        std::env::var("OPENSEARCH_USER"),
        std::env::var("OPENSEARCH_PASS"),
    ) {
        (Ok(u), Ok(p)) => Some((u, p)),
        _ => None,
    };
    let monitor = json!({
        "type": "monitor",
        "name": name,
        "monitor_type": "query_level_monitor",
        "enabled": true,
        "schedule": {"period": {"interval": 1, "unit": "MINUTES"}},
        "inputs": [{"search": {"indices": [BACKTEST_INDEX], "query": {
            "size": 0,
            "query": {"bool": {
                "must": [{"query_string": {"query": lucene}}],
                "filter": [{"range": {"@timestamp": {"gte": "now-1m"}}}]
            }}
        }}}],
        "triggers": [{"name": format!("{name} trigger"), "severity": "1",
            "condition": {"script": {"source": "ctx.results[0].hits.total.value > 0", "lang": "painless"}},
            "actions": []}]
    });
    let mut req = http.post(format!(
        "{}/_plugins/_alerting/monitors",
        base.trim_end_matches('/')
    ));
    if let Some((u, p)) = auth {
        req = req.basic_auth(u, Some(p));
    }
    req.json(&monitor).send().await?.error_for_status()?;
    Ok(())
}

/// Audits the deployment by invoking `detection.sigma.deploy` through the
/// shared gateway. Failures are logged, not fatal — the tool server may be
/// down, and a denial is recorded by the tool server's own audit chain.
pub async fn audit_deploy(
    gateway: &agent_core::HttpToolGateway,
    rule_path: &Path,
    correlation_id: Uuid,
) {
    let call = ToolCall {
        id: Uuid::new_v4(),
        tool: "detection.sigma.deploy".to_string(),
        args: json!({"rule_path": rule_path.to_string_lossy(), "backtest_fp_max": 5}),
        actor: "detection-agent".to_string(),
        idempotency_key: format!("sigma-deploy:{}", rule_path.display()),
        correlation_id,
    };
    match gateway.invoke_verdict(&call).await {
        Ok(v) if v.decision.kind == agent_protocol::tool::DecisionKind::Allow => {
            info!(output = %v.result.output, "detection.sigma.deploy audited");
        }
        Ok(v) => warn!(
            reason = %v.decision.reason,
            "detection.sigma.deploy denied; deployment recorded as unaudited"
        ),
        Err(e) => warn!(error = %e, "tool-server unreachable for audit"),
    }
}

/// Whether `bin` resolves on PATH.
fn which(bin: &str) -> Option<PathBuf> {
    std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths)
            .map(|p| p.join(bin))
            .find(|p| p.is_file())
    })
}

/// Slugifies a rule title for use as a file name.
fn slugify(title: &str) -> String {
    title
        .to_ascii_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn validate_sigma_requires_the_three_keys() {
        let missing = "title: x\ndetection:\n  selection: {a: b}\n";
        assert!(validate_sigma(missing).is_err(), "logsource is required");

        let ok = "title: x\nlogsource: {product: windows}\ndetection:\n  selection: {a: b}\n";
        let doc = validate_sigma(ok).expect("valid rule");
        assert_eq!(doc["title"], "x");
    }

    #[test]
    fn naive_lucene_ands_scalar_terms() {
        let doc = json!({"detection": {"selection": {"a": "1", "b": "2"}}});
        let q = naive_lucene(&doc).expect("mapped");
        assert!(q.contains("a:1") && q.contains("b:2") && q.contains(" AND "));
    }

    #[test]
    fn naive_lucene_rejects_empty_selection() {
        let doc = json!({"detection": {"selection": {}}});
        assert!(naive_lucene(&doc).is_err());
    }

    #[test]
    fn slugify_lowercases_and_hyphenates() {
        assert_eq!(slugify("Suspicious PowerShell Encoded Command"), "suspicious-powershell-encoded-command");
        assert_eq!(slugify("  Odd / Chars -- Here!  "), "odd-chars-here");
    }
}
