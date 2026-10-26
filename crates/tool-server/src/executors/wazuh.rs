//! Wazuh agent listing and active-response executors.

use anyhow::{Context, Result};
use async_trait::async_trait;
use serde_json::Value;

use super::{arg_str, dry_run, Tool};
use connectors::WazuhClient;

/// `wazuh.agent.list`.
pub(crate) struct WazuhAgents(pub(crate) Option<WazuhClient>);

#[async_trait]
impl Tool for WazuhAgents {
    fn name(&self) -> &'static str {
        "wazuh.agent.list"
    }
    async fn execute(&self, _args: &Value) -> Result<Value> {
        let Some(c) = &self.0 else {
            return dry_run(_args);
        };
        c.list_agents().await
    }
}

/// `wazuh.active_response.isolate_host` / `kill_process` — Wazuh AR commands.
pub(crate) struct WazuhActiveResponse {
    pub(crate) name: &'static str,
    pub(crate) command: &'static str,
    pub(crate) client: Option<WazuhClient>,
}

#[async_trait]
impl Tool for WazuhActiveResponse {
    fn name(&self) -> &'static str {
        self.name
    }
    async fn execute(&self, args: &Value) -> Result<Value> {
        let Some(c) = &self.client else {
            return dry_run(args);
        };
        let agent_id = arg_str(args, &["agent_id"]).context("missing agent_id")?;
        let mut ar_args: Vec<String> = Vec::new();
        if let Some(d) = args.get("duration_minutes").and_then(|v| v.as_i64()) {
            ar_args.push(d.to_string());
        }
        if let Some(p) = args.get("pid").and_then(|v| v.as_i64()) {
            ar_args.push(p.to_string());
        }
        c.active_response(agent_id, self.command, ar_args).await
    }
}
