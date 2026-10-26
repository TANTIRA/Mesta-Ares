//! Integration surface: the shipped policy files load and behave as the
//! documented matrix says.

use std::path::PathBuf;

use policy_engine::{DecisionKind, EvalCtx, PolicyEngine};
use serde_json::json;
use uuid::Uuid;

fn policies_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../policies")
        .canonicalize()
        .expect("policies dir")
}

fn call(tool: &str, args: serde_json::Value) -> agent_protocol::tool::ToolCall {
    agent_protocol::tool::ToolCall {
        id: Uuid::new_v4(),
        tool: tool.to_string(),
        args,
        actor: "integration-test".to_string(),
        idempotency_key: String::new(),
        correlation_id: Uuid::new_v4(),
    }
}

#[test]
fn shipped_policies_load_and_cover_the_response_tools() {
    let engine = PolicyEngine::load(&policies_dir()).expect("load policies");
    let names = engine.tool_names();
    for required in [
        "opensearch.query",
        "thehive.case.create",
        "thehive.observable.add",
        "blocklist.ip.add",
        "blocklist.ip.remove",
        "wazuh.active_response.isolate_host",
        "wazuh.active_response.unisolate_host",
        "entra.user.disable",
        "entra.user.enable",
        "detection.sigma.deploy",
        "exec.sandbox",
    ] {
        assert!(names.contains(&required.to_string()), "{required} missing");
    }
}

#[test]
fn deny_by_default_holds_for_unlisted_tools() {
    let engine = PolicyEngine::load(&policies_dir()).expect("load policies");
    let mut ctx = EvalCtx::new();
    let d = engine.evaluate(&call("totally.unlisted.tool", json!({})), &mut ctx);
    assert!(matches!(d.kind, DecisionKind::Deny));
}

#[test]
fn misp_writes_still_require_approval() {
    let engine = PolicyEngine::load(&policies_dir()).expect("load policies");
    let mut ctx = EvalCtx::new();
    let d = engine.evaluate(
        &call("misp.ioc.block", json!({"value": "203.0.113.9"})),
        &mut ctx,
    );
    assert!(matches!(d.kind, DecisionKind::ApprovalRequired), "{d:?}");
}
