//! Triage agent for the Ares SIEM+SOAR pipeline.
//!
//! Consumes [`SUBJECT_TRIAGE`] envelopes and, per alert:
//! 1. Extracts IOCs via [`enrichment_lib::extract_iocs`].
//! 2. Enriches each IOC (cache-backed [`Enricher`]) and computes
//!    [`enrichment_lib::composite_score`].
//! 3. Asks the Fast LLM tier for a strict-JSON verdict
//!    (`tp` | `fp` | `suspicious`, severity, rationale).
//! 4. Checks the kill switch before any write.
//! 5. `fp`   → writes the verdict doc to OpenSearch `ares-verdicts`.
//!    `tp`/`suspicious` → opens a TheHive case + observables, and publishes a
//!    [`HandoffPayload`] to [`SUBJECT_RESPOND`] when severity ≥ high.
//!
//! Progress is emitted to `ares.activity` for the console SSE feed.
//!
//! NOTE: the false-positive verdict doc is written with a raw `POST
//! {OPENSEARCH_URL}/ares-verdicts/_doc` because [`OpensearchClient`] only
//! exposes `_search`.


use std::sync::Arc;

use agent_core::kill_switch_engaged;
use agent_protocol::{
    envelope::{Envelope, EnvelopeKind},
    handoff::{Artifact, HandoffPayload},
    SUBJECT_RESPOND, SUBJECT_TRIAGE,
};
use anyhow::Context;
use clap::Parser;
use connectors::{ConnectorConfig, NewCase, NewObservable, OpensearchClient};
use enrichment_lib::{composite_score, extract_iocs, Enricher, IocSet};
use futures::StreamExt;
use llm_client::{
    fast_model, AnthropicProvider, CompletionRequest, LlmProvider, Message, ModelRouter, ModelTier,
    Role, SystemOneClient,
};
use serde::Deserialize;
use serde_json::{json, Value};
use tracing::{error, info, warn, Instrument};

use agent_core::{publish_activity, HttpToolGateway};
use agent_protocol::activity::ActivityEvent;
use agent_protocol::severity::Severity;
use agent_protocol::tool::ToolCall;

/// Triage agent service configuration.
#[derive(Debug, Parser)]
#[command(name = "triage-agent", about = "Ares alert triage agent")]
struct Config {
    /// NATS server URL.
    #[arg(long, env = "NATS_URL", default_value = "nats://localhost:4222")]
    nats_url: String,

    /// Tool-server base URL (every side effect goes through its policy gate).
    #[arg(long, env = "TOOL_SERVER_URL", default_value = "http://localhost:8090")]
    tool_server_url: String,
}

/// Verdict class; anything unrecognised is treated as `suspicious` rather
/// than dropping the alert.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum VerdictKind {
    /// True positive: malicious, needs response.
    Tp,
    /// False positive: benign or expected.
    Fp,
    /// Inconclusive; keep watching.
    Suspicious,
}

impl VerdictKind {
    /// Parses the wire spelling; unknown values become [`VerdictKind::Suspicious`].
    fn parse(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "tp" => Self::Tp,
            "fp" => Self::Fp,
            _ => Self::Suspicious,
        }
    }

    /// Stable lowercase spelling.
    fn as_str(self) -> &'static str {
        match self {
            Self::Tp => "tp",
            Self::Fp => "fp",
            Self::Suspicious => "suspicious",
        }
    }
}

impl<'de> Deserialize<'de> for VerdictKind {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(d)?;
        Ok(Self::parse(&raw))
    }
}

impl serde::Serialize for VerdictKind {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.as_str())
    }
}

/// LLM verdict parsed from the strict-JSON completion.
#[derive(Debug, Deserialize)]
struct Verdict {
    /// `tp` (true positive), `fp` (false positive), or `suspicious`.
    verdict: VerdictKind,
    /// `low` | `medium` | `high` | `critical`.
    severity: Severity,
    /// One-paragraph justification.
    rationale: String,
}

/// Severity ranks that trigger a handoff to the response agent.
fn severity_gte_high(severity: Severity) -> bool {
    severity >= Severity::High
}

/// Strict-JSON system prompt for the verdict completion.
const VERDICT_PROMPT: &str = r#"You are a SOC triage analyst. Given an alert, its extracted IOCs,
enrichment results and a composite threat score (0-100), return ONLY a JSON
object — no prose, no markdown — with exactly these keys:
{"verdict":"tp|fp|suspicious","severity":"low|medium|high|critical","rationale":"one paragraph"}
"tp" = true positive needing response, "fp" = benign false positive,
"suspicious" = likely malicious but needs monitoring."#;


/// Lists every IOC in the set paired with its TheHive `dataType`.
fn typed_iocs(set: &IocSet) -> Vec<(&'static str, &String)> {
    let mut v: Vec<(&'static str, &String)> = Vec::new();
    v.extend(set.ips.iter().map(|i| ("ip", i)));
    v.extend(set.domains.iter().map(|i| ("domain", i)));
    v.extend(set.urls.iter().map(|i| ("url", i)));
    v.extend(set.hashes_md5_sha256.iter().map(|i| ("hash", i)));
    v.extend(set.emails.iter().map(|i| ("email", i)));
    v
}

/// Asks the Fast model for a [`Verdict`] for one alert.
async fn llm_verdict(router: &ModelRouter, context: &Value) -> anyhow::Result<Verdict> {
    let req = CompletionRequest {
        model: fast_model(),
        system: Some(VERDICT_PROMPT.to_string()),
        messages: vec![Message {
            role: Role::User,
            content: serde_json::to_string(context)?,
        }],
        max_tokens: 512,
        temperature: Some(0.0),
    };
    let resp = router.complete(ModelTier::Fast, &req).await?;
    let text = resp
        .text
        .trim()
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim()
        .to_string();
    let verdict: Verdict = serde_json::from_str(&text).context("parse LLM verdict JSON")?;
    Ok(verdict)
}

/// Verdict labels and their decision-model descriptions.
const VERDICT_LABELS: &[(&str, &str)] = &[
    ("tp", "True positive: malicious activity needing response"),
    ("fp", "False positive: benign or expected activity"),
    (
        "suspicious",
        "Suspicious: likely malicious but not confirmed, needs monitoring",
    ),
];

/// Severity levels, lowest first, for the decision-model `score` question.
const SEVERITY_LEVELS: &[&str] = &[
    "low: informational, no operational impact",
    "medium: limited impact or unconfirmed scope",
    "high: confirmed compromise or active threat",
    "critical: widespread or destructive impact",
];

/// Maps a decision-model severity level (0–3, probability-weighted) to a
/// [`Severity`], clamping out-of-range values instead of downgrading them.
fn severity_from_level(level: f32) -> Severity {
    const LEVELS: [Severity; 4] = [
        Severity::Low,
        Severity::Medium,
        Severity::High,
        Severity::Critical,
    ];
    LEVELS[level.round().clamp(0.0, 3.0) as usize]
}

/// Asks a local Ollama decision model for the verdict: a `choice` over
/// tp/fp/suspicious plus a `score` over the four severity levels. The
/// rationale is synthesized from the returned probabilities — decision models
/// produce labels and scores, not prose.
async fn decision_verdict(
    client: &SystemOneClient,
    model: &str,
    context: &Value,
) -> anyhow::Result<Verdict> {
    let choice = client
        .choice(
            model,
            context,
            "Which verdict fits this alert, its IOCs and their enrichment?",
            VERDICT_LABELS,
        )
        .await
        .context("decision model choice")?;
    let level = client
        .score(
            model,
            context,
            "How severe is this alert if the verdict is a true positive?",
            SEVERITY_LEVELS,
        )
        .await
        .context("decision model score")?;

    let severity = severity_from_level(level);
    let probs = VERDICT_LABELS
        .iter()
        .filter_map(|(label, _)| {
            choice
                .probabilities
                .get(*label)
                .map(|p| format!("{label}={p:.2}"))
        })
        .collect::<Vec<_>>()
        .join(", ");
    Ok(Verdict {
        verdict: VerdictKind::parse(&choice.choice),
        severity,
        rationale: format!(
            "local decision model {model}: verdict {} (confidence {:.2}; {probs}); severity score {level:.2}/3",
            VerdictKind::parse(&choice.choice).as_str(),
            choice.confidence
        ),
    })
}

/// Decision-model verdict when `DECISION_MODEL` is set, else the LLM; a
/// decision-model failure falls back to the LLM rather than dropping the alert.
async fn verdict(deps: &Deps, context: &Value) -> anyhow::Result<Verdict> {
    if let (Some(client), Some(model)) = (&deps.decision, &deps.decision_model) {
        match decision_verdict(client, model, context).await {
            Ok(v) => return Ok(v),
            Err(e) => warn!(error = %e, "decision-model verdict failed; falling back to LLM"),
        }
    }
    llm_verdict(&deps.router, context).await
}

/// Writes a false-positive verdict document to OpenSearch `ares-verdicts`.
async fn write_fp_verdict(
    os: &OpensearchClient,
    alert: &Value,
    verdict: &Verdict,
) -> anyhow::Result<()> {
    let doc = json!({
        "alert": alert,
        "verdict": verdict.verdict,
        "severity": verdict.severity,
        "rationale": verdict.rationale,
        "ts": chrono::Utc::now(),
    });
    os.index_doc("ares-verdicts", &doc).await?;
    Ok(())
}

/// Builds a governed [`ToolCall`] for a side effect this agent owns.
fn tool_call(tool: &str, args: Value, idempotency_key: String, run_id: uuid::Uuid) -> ToolCall {
    ToolCall {
        id: uuid::Uuid::new_v4(),
        tool: tool.to_string(),
        args,
        actor: "triage-agent".to_string(),
        idempotency_key,
        correlation_id: run_id,
    }
}

/// Opens a TheHive case and attaches each IOC as an observable.
async fn open_case(
    gateway: &HttpToolGateway,
    alert: &Value,
    verdict: &Verdict,
    iocs: &[(&'static str, &String)],
    run_id: uuid::Uuid,
) -> anyhow::Result<String> {
    let title = alert
        .get("rule")
        .and_then(|r| r.get("description").or_else(|| r.get("id")))
        .and_then(Value::as_str)
        .unwrap_or("Ares triage case")
        .to_string();
    let severity = verdict.severity.rank();
    let case = NewCase {
        title: title.clone(),
        description: verdict.rationale.clone(),
        severity,
        tags: vec!["ares".to_string(), "triaged".to_string()],
        tlp: 2,
    };
    let created = gateway
        .invoke_allowed(&tool_call(
            "thehive.case.create",
            serde_json::to_value(&case)?,
            format!("triage-case:{title}"),
            run_id,
        ))
        .await?;
    let case_id = created
        .get("id")
        .and_then(Value::as_str)
        .context("thehive.case.create returned no id")?
        .to_string();

    for (data_type, ioc) in iocs {
        let obs = NewObservable {
            data_type: (*data_type).to_string(),
            data: (*ioc).clone(),
            message: None,
            tags: vec!["ares".to_string()],
            tlp: Some(2),
            ioc: true,
        };
        let call = tool_call(
            "thehive.observable.add",
            json!({"case_id": case_id, "observable": obs}),
            format!("triage-obs:{case_id}:{ioc}"),
            run_id,
        );
        if let Err(e) = gateway.invoke_allowed(&call).await {
            warn!(error = %e, case_id, "observable add failed");
        }
    }
    Ok(case_id)
}

/// Publishes a [`HandoffPayload`] to [`SUBJECT_RESPOND`] for response-agent.
async fn handoff_respond(
    client: &async_nats::Client,
    envelope: &Envelope,
    case_id: &str,
    verdict: &Verdict,
) -> anyhow::Result<()> {
    let handoff = HandoffPayload {
        goal: format!(
            "Respond to {severity} severity case {case_id}",
            severity = verdict.severity
        ),
        constraints: vec!["respect policies/tools.yml allowlist".to_string()],
        artifacts: vec![Artifact {
            kind: "case".to_string(),
            ref_uri: format!("thehive://case/{case_id}"),
            summary: verdict.rationale.clone(),
        }],
        open_questions: vec![],
    };
    let mut env = Envelope::new(
        "triage-agent",
        EnvelopeKind::Case,
        serde_json::to_value(&handoff)?,
    );
    env.correlation_id = envelope.correlation_id;
    let bytes = serde_json::to_vec(&env)?;
    client
        .publish(SUBJECT_RESPOND.to_string(), bytes.into())
        .await?;
    Ok(())
}

/// Shared clients threaded through each spawned task.
struct Deps {
    nats: async_nats::Client,
    enricher: Arc<Enricher>,
    router: Arc<ModelRouter>,
    /// Local decision model used for the verdict when `DECISION_MODEL` is set.
    decision: Option<SystemOneClient>,
    /// Decision-model name (e.g. `tev1:4b`); `None` uses the LLM path.
    decision_model: Option<String>,
    gateway: Arc<HttpToolGateway>,
    /// Verdict documents are indexed here.
    os: Arc<OpensearchClient>,
}

/// Processes one triage envelope end-to-end.
async fn process(deps: &Deps, envelope: &Envelope) -> anyhow::Result<()> {
    let run_id = envelope.correlation_id;
    publish_activity(
        &deps.nats,
        &ActivityEvent::new(run_id, "triage", "received", "alert received"),
    )
    .await;

    // 1. IOC extraction over the raw alert JSON.
    let alert_text = serde_json::to_string(&envelope.payload)?;
    let ioc_set = extract_iocs(&alert_text);
    let iocs = typed_iocs(&ioc_set);
    publish_activity(
        &deps.nats,
        &ActivityEvent::new(
            run_id,
            "triage",
            "extract_iocs",
            format!("{} IOCs", iocs.len()),
        ),
    )
    .await;

    // 2. Enrichment + composite score.
    let mut results = Vec::new();
    for &(_, ioc) in &iocs {
        results.extend(deps.enricher.enrich(ioc).await);
    }
    let score = composite_score(&results);
    publish_activity(
        &deps.nats,
        &ActivityEvent::new(run_id, "triage", "enrich", format!("score={score}")),
    )
    .await;

    // 2b. CVEs referenced by the alert get NVD context (cache-backed).
    let mut cve_recs = Vec::new();
    for cve_id in &ioc_set.cve_ids {
        if let Some(rec) = deps.enricher.enrich_cve(cve_id).await {
            cve_recs.push(rec);
        }
    }

    // 3. Verdict: local decision model when configured, else the Fast LLM.
    let context = json!({
        "alert": envelope.payload,
        "iocs": iocs.iter().map(|(t, i)| json!({"type": t, "value": i})).collect::<Vec<_>>(),
        "enrichment": results,
        "cves": cve_recs,
        "composite_score": score,
    });
    let verdict = verdict(deps, &context).await?;
    publish_activity(
        &deps.nats,
        &ActivityEvent::new(
            run_id,
            "triage",
            "verdict",
            format!("{} ({})", verdict.verdict.as_str(), verdict.severity),
        ),
    )
    .await;

    // 4. Kill switch gates every write path.
    if kill_switch_engaged() {
        warn!("kill switch engaged; skipping writes");
        publish_activity(
            &deps.nats,
            &ActivityEvent::new(run_id, "triage", "kill_switch", "writes suppressed"),
        )
        .await;
        return Ok(());
    }

    // 5. Route on verdict.
    match verdict.verdict {
        VerdictKind::Fp => {
            write_fp_verdict(&deps.os, &envelope.payload, &verdict).await?;
            publish_activity(
                &deps.nats,
                &ActivityEvent::new(run_id, "triage", "close", "false positive recorded"),
            )
            .await;
        }
        VerdictKind::Tp | VerdictKind::Suspicious => {
            let mut observables = iocs.clone();
            observables.extend(ioc_set.cve_ids.iter().map(|c| ("cve", c)));
            let case_id = open_case(&deps.gateway, &envelope.payload, &verdict, &observables, run_id).await?;
            publish_activity(
                &deps.nats,
                &ActivityEvent::new(run_id, "triage", "case", format!("case {case_id} opened")),
            )
            .await;
            if severity_gte_high(verdict.severity) {
                handoff_respond(&deps.nats, envelope, &case_id, &verdict).await?;
                publish_activity(
                    &deps.nats,
                    &ActivityEvent::new(run_id, "triage", "handoff", "handed to response-agent"),
                )
                .await;
            }
        }
    }
    Ok(())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let config = Config::parse();
    let nats = async_nats::connect(&config.nats_url).await?;
    let conn_cfg = ConnectorConfig::from_env();
    let provider: Arc<dyn LlmProvider> = Arc::new(AnthropicProvider::from_env()?);
    let deps = Arc::new(Deps {
        nats: nats.clone(),
        enricher: Arc::new(Enricher::new()),
        router: Arc::new(ModelRouter::new(provider.clone(), provider)),
        decision: Some(SystemOneClient::from_env()),
        decision_model: std::env::var("DECISION_MODEL")
            .ok()
            .filter(|s| !s.is_empty()),
        gateway: Arc::new(HttpToolGateway::new(config.tool_server_url.clone())),
        os: Arc::new(
            conn_cfg
                .build_opensearch_client()
                .context("OPENSEARCH_URL not set")?,
        ),
    });

    let mut sub = nats.subscribe(SUBJECT_TRIAGE.to_string()).await?;
    info!(subject = SUBJECT_TRIAGE, "triage-agent subscribed");

    while let Some(msg) = sub.next().await {
        let deps = deps.clone();
        tokio::spawn(async move {
            let result: anyhow::Result<()> = async {
                let envelope: Envelope =
                    serde_json::from_slice(&msg.payload).context("parse envelope")?;
                let span = tracing::info_span!(
                    "triage",
                    workflow_run_id = %envelope.correlation_id,
                    envelope_id = %envelope.id,
                );
                process(&deps, &envelope).instrument(span).await
            }
            .await;
            if let Err(e) = result {
                error!(error = %e, "triage processing failed");
            }
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Weighted level rounds to the nearest severity label and clamps.
    #[test]
    fn severity_levels_map_to_labels() {
        assert_eq!(severity_from_level(0.2), Severity::Low);
        assert_eq!(severity_from_level(1.2), Severity::Medium);
        assert_eq!(severity_from_level(1.83), Severity::High);
        assert_eq!(severity_from_level(2.9), Severity::Critical);
        assert_eq!(severity_from_level(9.0), Severity::Critical);
        assert_eq!(severity_from_level(-1.0), Severity::Low);
    }

    /// High and critical trigger the response-agent handoff.
    #[test]
    fn handoff_threshold_is_high() {
        assert!(severity_gte_high(Severity::High));
        assert!(severity_gte_high(Severity::Critical));
        assert!(!severity_gte_high(Severity::Medium));
        assert!(!severity_gte_high(Severity::Low));
    }
}
