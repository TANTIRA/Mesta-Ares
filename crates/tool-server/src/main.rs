//! tool-server: the only process holding side-effect credentials.
//!
//! `POST /invoke` runs every [`ToolCall`] through the [`PolicyEngine`], audits
//! the decision, and dispatches allowed calls to the executor registry.
//! Denials return HTTP 200 with the verdict in the response body — the
//! `decision` field is the contract, not the status code.
//!
//! Calls flagged `approval_required` are parked in the approval queue
//! (`GET /approvals`, `POST /approvals/{id}/approve|reject`); approving
//! re-checks policy and executes the stored call.

mod approvals;
mod executors;
mod idempotency;

use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;

use agent_protocol::tool::{Decision, DecisionKind, ToolCall, ToolResult};
use anyhow::{Context, Result};
use audit::AuditWriter;
use axum::{extract::State, http::StatusCode, routing::get, routing::post, Json, Router};
use connectors::ConnectorConfig;
use executors::Tool;
use policy_engine::{EvalCtx, PolicyEngine};
use serde::Serialize;
use serde_json::Value;
use tokio::sync::Mutex;
use tracing::{error, info, warn};
use uuid::Uuid;

/// `POST /invoke` response: verdict plus execution result.
#[derive(Serialize)]
struct InvokeResponse {
    decision: Decision,
    result: ToolResult,
    /// Approval id when the call was parked for human approval.
    #[serde(skip_serializing_if = "Option::is_none")]
    approval_id: Option<Uuid>,
}

struct AppState {
    engine: PolicyEngine,
    registry: HashMap<String, Arc<dyn Tool>>,
    audit: Option<AuditWriter>,
    /// Three-state idempotency records keyed by `idempotency_key`.
    idem: idempotency::Store,
    /// Calls parked on `autonomy: approval`.
    approvals: approvals::Store,
    eval_ctx: Mutex<EvalCtx>,
}

/// Prefer the caller-supplied idempotency key; absent one, no caching.
fn idem_key(call: &ToolCall) -> Option<String> {
    (!call.idempotency_key.is_empty()).then(|| call.idempotency_key.clone())
}

async fn invoke(
    State(state): State<Arc<AppState>>,
    axum::extract::Extension(actor): axum::extract::Extension<http_limits::auth::Actor>,
    Json(mut call): Json<ToolCall>,
) -> (StatusCode, Json<InvokeResponse>) {
    // The body's actor is advisory; the authenticated identity wins so policy
    // and audit see a verified caller.
    call.actor = actor.0;
    // Rename alias argument keys to their canonical spelling (policies/tools.yml
    // owns the mapping) before the schema check sees them.
    state.engine.canonicalize(&mut call);
    info!(tool = %call.tool, actor = %call.actor, "invoke received");

    let decision = {
        let mut ctx = state.eval_ctx.lock().await;
        ctx.refresh_kill_switch();
        state.engine.evaluate(&call, &mut ctx)
    };

    // Every decision is audited; an audit failure degrades to a log line.
    if let Some(writer) = &state.audit {
        if let Err(e) = writer
            .append(
                &call.actor,
                &call.tool,
                &call.args,
                decision.kind,
                &decision.reason,
                call.correlation_id,
            )
            .await
        {
            error!(error = %e, "audit append failed");
        }
    } else {
        warn!("no audit writer configured (DATABASE_URL unset)");
    }

    let (result, approval_id) = match &decision.kind {
        DecisionKind::Allow => (claim_and_execute(&state, &call).await, None),
        DecisionKind::Deny => (
            ToolResult {
                call_id: call.id,
                ok: false,
                output: Value::Null,
                error: Some(format!("denied: {}", decision.reason)),
                dry_run: false,
            },
            None,
        ),
        DecisionKind::ApprovalRequired => {
            // Park the call so a human can approve or reject it later.
            let id = state.approvals.insert(&call, &decision.reason);
            (
                ToolResult {
                    call_id: call.id,
                    ok: false,
                    output: Value::Null,
                    error: Some(format!(
                        "approval required: {} (approval id {id})",
                        decision.reason
                    )),
                    dry_run: false,
                },
                Some(id),
            )
        }
    };

    (
        StatusCode::OK,
        Json(InvokeResponse {
            decision,
            result,
            approval_id,
        }),
    )
}

pub(crate) async fn claim_and_execute(state: &Arc<AppState>, call: &ToolCall) -> ToolResult {
    // No key: nothing to dedupe, run straight through.
    let Some(key) = idem_key(call) else {
        return run_tool(state, call).await;
    };
    let fp = idempotency::fingerprint(call);
    match state.idem.claim(&key, &call.tool, &fp).await {
        Ok(idempotency::Claim::Execute) => {
            let result = run_tool(state, call).await;
            if let Err(e) = state.idem.finish(&key, &result).await {
                error!(key = %key, error = %e, "idempotency finish failed");
            }
            result
        }
        Ok(idempotency::Claim::Replay(result)) => {
            info!(key = %key, "idempotent replay, returning stored result");
            result
        }
        Ok(idempotency::Claim::Mismatch) => {
            // 409-equivalent: HTTP stays 200, `decision` still carries the
            // policy verdict, the conflict lands in `result.error`.
            warn!(key = %key, "idempotency payload mismatch");
            ToolResult {
                call_id: call.id,
                ok: false,
                output: Value::Null,
                error: Some(idempotency::ERR_MISMATCH.to_string()),
                dry_run: false,
            }
        }
        Ok(idempotency::Claim::InFlight) => {
            info!(key = %key, "idempotency key already in flight");
            ToolResult::retryable(call.id, idempotency::ERR_IN_FLIGHT)
        }
        Err(e) => {
            // Fail closed: executing without the record would double-run on
            // replay, so the caller gets a retryable error instead.
            error!(key = %key, error = %e, "idempotency store error");
            ToolResult::retryable(call.id, idempotency::ERR_STORE)
        }
    }
}

async fn run_tool(state: &Arc<AppState>, call: &ToolCall) -> ToolResult {
    let Some(tool) = state.registry.get(&call.tool) else {
        return ToolResult {
            call_id: call.id,
            ok: false,
            output: Value::Null,
            error: Some(format!("no executor registered for `{}`", call.tool)),
            dry_run: false,
        };
    };

    match tool.execute(&call.args).await {
        Ok(output) => {
            // Executors that lack credentials return a `dry_run` payload.
            let dry_run = output
                .get("dry_run")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            ToolResult {
                call_id: call.id,
                ok: true,
                output,
                error: None,
                dry_run,
            }
        }
        Err(e) => ToolResult {
            call_id: call.id,
            ok: false,
            output: Value::Null,
            error: Some(format!("{e:#}")),
            dry_run: false,
        },
    }
}

async fn tools(State(state): State<Arc<AppState>>) -> Json<Vec<String>> {
    Json(state.engine.tool_names())
}

async fn health() -> StatusCode {
    StatusCode::OK
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,tool_server=debug".into()),
        )
        .init();

    let policies_dir = std::env::var("POLICIES_DIR").unwrap_or_else(|_| "policies".to_string());
    let engine = PolicyEngine::load(Path::new(&policies_dir))
        .with_context(|| format!("load policies from {policies_dir}"))?;

    let cfg = ConnectorConfig::from_env();
    let registry = executors::build_registry(&cfg);
    let missing = executors::missing_executors(&registry, &engine.tool_names());
    if !missing.is_empty() {
        anyhow::bail!("allowlisted tools have no executor: {missing:?}; refusing to start");
    }

    let pg = match std::env::var("DATABASE_URL") {
        Ok(url) if !url.is_empty() => match sqlx::PgPool::connect(&url).await {
            Ok(pg) => Some(pg),
            Err(e) => {
                error!(error = %e, "DATABASE_URL connect failed; running without audit");
                None
            }
        },
        _ => {
            warn!("DATABASE_URL unset; running without audit or durable idempotency");
            None
        }
    };

    if let Some(pool) = &pg {
        if let Err(e) = AuditWriter::ensure_schema(pool).await {
            error!(error = %e, "audit_log schema init failed");
        }
    }
    let audit = pg.clone().map(AuditWriter::new);
    let idem = match &pg {
        Some(pool) => {
            if let Err(e) = idempotency::ensure_schema(pool).await {
                // Claims will fail closed (retryable errors) until the table exists.
                error!(error = %e, "tool_idempotency schema init failed");
            }
            idempotency::Store::pg(pool.clone())
        }
        None => {
            warn!("idempotency records are in-memory only");
            idempotency::Store::mem()
        }
    };

    let state = Arc::new(AppState {
        engine,
        registry,
        audit,
        idem,
        approvals: approvals::Store::new(),
        eval_ctx: Mutex::new(EvalCtx::new()),
    });

    let app = Router::new()
        .route("/health", get(health))
        .merge(
            Router::new()
                .route("/invoke", post(invoke))
                .route("/tools", get(tools))
                .route("/approvals", get(approvals::list))
                .route("/approvals/{id}/approve", post(approvals::approve))
                .route("/approvals/{id}/reject", post(approvals::reject))
                .layer(axum::middleware::from_fn_with_state(
                    http_limits::auth::ApiKeys::from_env(),
                    http_limits::auth::require_auth,
                )),
        )
        .with_state(state);

    let bind: SocketAddr = std::env::var("BIND_ADDR")
        .unwrap_or_else(|_| "0.0.0.0:8090".to_string())
        .parse()
        .context("invalid BIND_ADDR")?;
    let listener = tokio::net::TcpListener::bind(bind).await?;
    info!(addr = %bind, "tool-server listening");
    axum::serve(listener, app).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn policies_dir() -> PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../policies")
            .canonicalize()
            .expect("policies dir")
    }

    fn state() -> Arc<AppState> {
        let engine = PolicyEngine::load(&policies_dir()).expect("load policies");
        let registry = executors::build_registry(&ConnectorConfig::default());
        Arc::new(AppState {
            engine,
            registry,
            audit: None,
            idem: idempotency::Store::mem(),
            approvals: approvals::Store::new(),
            eval_ctx: Mutex::new(EvalCtx::new()),
        })
    }

    #[test]
    fn every_allowlisted_tool_has_an_executor() {
        let engine = PolicyEngine::load(&policies_dir()).expect("load policies");
        let registry = executors::build_registry(&ConnectorConfig::default());
        let missing = executors::missing_executors(&registry, &engine.tool_names());
        assert!(
            missing.is_empty(),
            "allowlisted tools without executor: {missing:?}"
        );
    }

    #[tokio::test]
    async fn claim_and_execute_runs_the_registered_tool() {
        let s = state();
        let call = ToolCall {
            id: Uuid::new_v4(),
            tool: "cvss.score".to_string(),
            args: serde_json::json!({"vector": "CVSS:3.1/AV:N/AC:L/PR:N/UI:N/S:U/C:H/I:H/A:H"}),
            actor: "test".to_string(),
            idempotency_key: "invoke-path-test".to_string(),
            correlation_id: Uuid::new_v4(),
        };
        let first = claim_and_execute(&s, &call).await;
        assert!(first.ok, "{first:?}");
        assert!(first.output.get("score").is_some(), "{:?}", first.output);

        // Same key replays the stored result instead of re-running.
        let replay = claim_and_execute(&s, &call).await;
        assert!(replay.ok);
        assert_eq!(replay.output, first.output);
    }
}
