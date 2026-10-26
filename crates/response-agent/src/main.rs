//! Response agent for the Ares SIEM+SOAR pipeline.
//!
//! Consumes [`SUBJECT_RESPOND`] envelopes carrying a [`HandoffPayload`] from
//! triage, asks the Strong LLM tier for an ordered action plan, and executes
//! each step through the tool server (`{TOOL_SERVER_URL}/invoke`) with an
//! idempotency key of `case_id:tool:sha256(args)`.
//!
//! Per action: invoke → post-check → append result to the TheHive case →
//! emit `ares.activity`. Reversible actions record a rollback entry in
//! Postgres `response_actions`. Denied actions are recorded and the plan
//! continues. The loop stops cleanly when [`kill_switch_engaged`] flips
//! mid-plan.
//!
//! The tool server answers `POST /invoke` with `{decision, result}`; a deny
//! is visible as `decision.kind == "deny"` with `result.error` carrying the
//! reason.


use std::sync::Arc;

use agent_core::kill_switch_engaged;
use agent_protocol::{
    envelope::Envelope,
    handoff::HandoffPayload,
    tool::{Decision, DecisionKind, ToolCall, ToolResult},
    SUBJECT_RESPOND,
};
use anyhow::Context;
use clap::Parser;
use connectors::{ConnectorConfig, ThehiveClient, WazuhClient};
use futures::StreamExt;
use llm_client::{
    AnthropicProvider, CompletionRequest, LlmProvider, Message, ModelRouter, ModelTier, Role,
    strong_model,
};
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use tracing::{error, info, warn, Instrument};
use uuid::Uuid;

use agent_core::publish_activity;
use agent_protocol::activity::ActivityEvent;

/// Response agent service configuration.
#[derive(Debug, Parser)]
#[command(name = "response-agent", about = "Ares autonomous response agent")]
struct Config {
    /// NATS server URL.
    #[arg(long, env = "NATS_URL", default_value = "nats://localhost:4222")]
    nats_url: String,

    /// Postgres connection URL for the `response_actions` table.
    #[arg(long, env = "DATABASE_URL")]
    database_url: String,

    /// Tool server base URL.
    #[arg(long, env = "TOOL_SERVER_URL", default_value = "http://localhost:8090")]
    tool_server_url: String,
}

/// One planned action from the Strong model.
#[derive(Debug, Clone, Deserialize)]
struct PlannedAction {
    /// Tool name, e.g. `wazuh.active_response.isolate_host`.
    tool: String,
    /// Tool arguments object.
    #[serde(default)]
    args: Value,
    /// Why this step is in the plan.
    #[serde(default)]
    rationale: String,
}

/// Strict-JSON plan prompt for the Strong tier.
const PLAN_PROMPT: &str = r#"You are an autonomous incident responder. Given the case context, return
ONLY a JSON array — no prose, no markdown — of ordered response actions:
[{"tool":"<name>","args":{...},"rationale":"why"}]
Only use tools justified by the evidence. Prefer reversible, scoped actions.
Available tools are the allowlist in policies/tools.yml (opensearch.query,
thehive.*, misp.*, blocklist.ip.add, blocklist.domain.add, wazuh.active_response.*,
entra.user.*, firewall.rule.add, o365.email.quarantine, notify.slack.post)."#;


/// Maps a tool to the rollback call that reverses it, when one exists.
///
/// Returns `(rollback_tool, rollback_args)` for the `response_actions`
/// `rollback_json` column. Irreversible or read-only tools return `None`.
fn rollback_for(action: &PlannedAction) -> Option<Value> {
    let args = &action.args;
    let (tool, rargs) = match action.tool.as_str() {
        "blocklist.ip.add" => ("blocklist.ip.remove", json!({"ip": args.get("ip")})),
        "blocklist.domain.add" => (
            "blocklist.domain.remove",
            json!({"domain": args.get("domain")}),
        ),
        "wazuh.active_response.isolate_host" => (
            "wazuh.active_response.unisolate_host",
            json!({"agent_id": args.get("agent_id")}),
        ),
        "entra.user.disable" => ("entra.user.enable", json!({"user_id": args.get("user_id")})),
        "ad.user.disable" => (
            "ad.user.enable",
            json!({"sam_account_name": args.get("sam_account_name")}),
        ),
        "firewall.rule.add" => (
            "firewall.rule.remove",
            json!({"remote": args.get("remote")}),
        ),
        "o365.email.quarantine" => (
            "o365.email.release",
            json!({"message_id": args.get("message_id")}),
        ),
        "misp.ioc.block" => ("misp.ioc.unblock", args.clone()),
        _ => return None,
    };
    Some(json!({"tool": tool, "args": rargs}))
}

/// Asks the Strong model for an ordered [`PlannedAction`] list.
async fn build_plan(router: &ModelRouter, context: &Value) -> anyhow::Result<Vec<PlannedAction>> {
    let req = CompletionRequest {
        model: strong_model(),
        system: Some(PLAN_PROMPT.to_string()),
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
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim()
        .to_string();
    serde_json::from_str(&text).context("parse LLM plan JSON")
}

/// Post-invocation sanity check for side-effecting actions.
///
/// For Wazuh active-response actions this lists agents so the result captures
/// the post-state of the fleet; other tools record a lightweight note.
async fn post_check(wazuh: &WazuhClient, action: &PlannedAction) -> Value {
    if action.tool.starts_with("wazuh.") {
        match wazuh.list_agents().await {
            Ok(agents) => json!({"check": "wazuh.list_agents", "agents": agents}),
            Err(e) => json!({"check": "wazuh.list_agents", "error": e.to_string()}),
        }
    } else {
        json!({"check": "skipped", "reason": "no post-check defined"})
    }
}

/// Extracts the TheHive case id from the handoff artifacts.
fn case_id_of(handoff: &HandoffPayload) -> String {
    handoff
        .artifacts
        .iter()
        .find(|a| a.kind == "case")
        .and_then(|a| {
            a.ref_uri
                .strip_prefix("thehive://case/")
                .map(str::to_string)
        })
        .unwrap_or_else(|| "unknown".to_string())
}

/// `POST /invoke` response shape returned by the tool server.
#[derive(Debug, Deserialize)]
struct InvokeResponse {
    /// Policy verdict on the call.
    decision: Decision,
    /// Execution result (or a denial stub).
    result: ToolResult,
}

/// Shared dependencies threaded into each spawned task.
struct Deps {
    nats: async_nats::Client,
    router: Arc<ModelRouter>,
    http: reqwest::Client,
    tool_url: String,
    hive: Arc<ThehiveClient>,
    wazuh: Arc<WazuhClient>,
    pool: PgPool,
}

/// POSTs `call` to `{tool_url}/invoke`; transport failures become a
/// `result.ok=false` response so the plan keeps moving.
async fn invoke_tool(deps: &Deps, call: &ToolCall) -> InvokeResponse {
    let url = format!("{}/invoke", deps.tool_url.trim_end_matches('/'));
    let fallback = |e: String| InvokeResponse {
        decision: Decision {
            kind: DecisionKind::Deny,
            reason: e.clone(),
            decided_at: chrono::Utc::now(),
        },
        result: ToolResult {
            call_id: call.id,
            ok: false,
            output: Value::Null,
            error: Some(e),
        },
    };
    match deps.http.post(&url).json(call).send().await {
        Ok(resp) => match resp.json::<InvokeResponse>().await {
            Ok(r) => r,
            Err(e) => fallback(format!("invoke response parse: {e}")),
        },
        Err(e) => fallback(format!("invoke transport: {e}")),
    }
}

/// Executes one response envelope end-to-end.
async fn process(deps: &Deps, envelope: &Envelope) -> anyhow::Result<()> {
    let run_id = envelope.correlation_id;
    let handoff: HandoffPayload =
        serde_json::from_value(envelope.payload.clone()).context("parse handoff payload")?;
    let case_id = case_id_of(&handoff);
    publish_activity(
        &deps.nats,
        &ActivityEvent::new(run_id, "response", "received", format!("case {case_id}")),
    )
    .await;

    // 1. Build the plan with the Strong tier.
    let context = json!({"case_id": case_id, "handoff": handoff});
    let plan = build_plan(&deps.router, &context).await?;
    publish_activity(
        &deps.nats,
        &ActivityEvent::new(
            run_id,
            "response",
            "plan",
            format!("{} actions planned", plan.len()),
        ),
    )
    .await;

    let mut results: Vec<Value> = Vec::new();
    for (idx, action) in plan.iter().enumerate() {
        if kill_switch_engaged() {
            warn!("kill switch engaged mid-plan; stopping");
            publish_activity(
                &deps.nats,
                &ActivityEvent::new(run_id, "response", "kill_switch", "plan halted"),
            )
            .await;
            break;
        }
        let step = format!("action_{idx}");
        publish_activity(
            &deps.nats,
            &ActivityEvent::new(run_id, "response", &step, format!("invoke {}", action.tool)),
        )
        .await;

        // 2. Invoke via the tool server with a deterministic idempotency key.
        let args_sha = hex(&Sha256::digest(&serde_json::to_vec(&action.args)?));
        let call = ToolCall {
            id: Uuid::new_v4(),
            tool: action.tool.clone(),
            args: action.args.clone(),
            actor: "response-agent".to_string(),
            idempotency_key: format!("{case_id}:{}:{args_sha}", action.tool),
            correlation_id: run_id,
        };
        let invoke = invoke_tool(deps, &call).await;
        let result = invoke.result;
        let denied = invoke.decision.kind == DecisionKind::Deny;

        // 3. Post-check on success; record everything to response_actions.
        let post = if result.ok {
            post_check(&deps.wazuh, action).await
        } else {
            Value::Null
        };
        let record = json!({
            "tool": action.tool,
            "args": action.args,
            "ok": result.ok,
            "decision": invoke.decision.kind,
            "decision_reason": invoke.decision.reason,
            "output": result.output,
            "error": result.error,
            "post_check": post,
            "rationale": action.rationale,
        });
        let rollback = if result.ok {
            rollback_for(action)
        } else {
            None
        };
        sqlx::query(
            "INSERT INTO response_actions (id, case_id, tool, args, result, rollback_json, ts)
             VALUES ($1, $2, $3, $4, $5, $6, now())",
        )
        .bind(Uuid::new_v4())
        .bind(&case_id)
        .bind(&action.tool)
        .bind(&action.args)
        .bind(&record)
        .bind(&rollback)
        .execute(&deps.pool)
        .await?;

        if denied {
            publish_activity(
                &deps.nats,
                &ActivityEvent::new(run_id, "response", &step, "denied by policy; recorded"),
            )
            .await;
        } else if !result.ok {
            publish_activity(
                &deps.nats,
                &ActivityEvent::new(run_id, "response", &step, "invocation failed; recorded"),
            )
            .await;
        }
        results.push(record);
    }

    // 4. Append the run summary to the TheHive case.
    if !results.is_empty() {
        if let Err(e) = deps
            .hive
            .update_case(&case_id, &json!({"ares_response_actions": results}))
            .await
        {
            warn!(error = %e, "thehive update failed");
        }
    }
    publish_activity(
        &deps.nats,
        &ActivityEvent::new(
            run_id,
            "response",
            "done",
            format!("{} actions executed", results.len()),
        ),
    )
    .await;
    Ok(())
}

/// Lowercase hex encoder for the idempotency key digest.
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let config = Config::parse();
    let nats = async_nats::connect(&config.nats_url).await?;
    let pool = PgPool::connect(&config.database_url).await?;
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS response_actions (
            id UUID PRIMARY KEY,
            case_id TEXT NOT NULL,
            tool TEXT NOT NULL,
            args JSONB NOT NULL,
            result JSONB,
            rollback_json JSONB,
            ts TIMESTAMPTZ NOT NULL DEFAULT now()
        )",
    )
    .execute(&pool)
    .await?;

    let conn_cfg = ConnectorConfig::from_env();
    let provider: Arc<dyn LlmProvider> = Arc::new(AnthropicProvider::from_env()?);
    let deps = Arc::new(Deps {
        nats: nats.clone(),
        router: Arc::new(ModelRouter::new(provider.clone(), provider)),
        http: reqwest::Client::new(),
        tool_url: config.tool_server_url.clone(),
        hive: Arc::new(conn_cfg.build_thehive_client().context("THEHIVE_URL not set")?),
        wazuh: Arc::new(conn_cfg.build_wazuh_client().context("WAZUH_API_URL not set")?),
        pool,
    });

    let mut sub = nats.subscribe(SUBJECT_RESPOND.to_string()).await?;
    info!(subject = SUBJECT_RESPOND, "response-agent subscribed");

    while let Some(msg) = sub.next().await {
        let deps = deps.clone();
        tokio::spawn(async move {
            let result: anyhow::Result<()> = async {
                let envelope: Envelope =
                    serde_json::from_slice(&msg.payload).context("parse envelope")?;
                let span = tracing::info_span!(
                    "response",
                    workflow_run_id = %envelope.correlation_id,
                    envelope_id = %envelope.id,
                );
                process(&deps, &envelope).instrument(span).await
            }
            .await;
            if let Err(e) = result {
                error!(error = %e, "response processing failed");
            }
        });
    }
    Ok(())
}
