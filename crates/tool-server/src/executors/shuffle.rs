//! Blocklist/firewall workflows through Shuffle.

use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;

use super::{dry_run, env, Tool};
use connectors::ShuffleClient;

/// Shuffle-workflow-backed executor (`firewall.rule.add`,
/// `blocklist.ip.add`, `blocklist.domain.add`). Workflow id comes from the
/// per-tool env var, falling back to `SHUFFLE_FW_WORKFLOW_ID`.
pub(crate) struct ShuffleWorkflow {
    pub(crate) name: &'static str,
    pub(crate) env_key: &'static str,
    pub(crate) client: Option<ShuffleClient>,
}

#[async_trait]
impl Tool for ShuffleWorkflow {
    fn name(&self) -> &'static str {
        self.name
    }
    async fn execute(&self, args: &Value) -> Result<Value> {
        let (Some(c), Some(wf)) = (
            &self.client,
            env(self.env_key).or_else(|| env("SHUFFLE_FW_WORKFLOW_ID")),
        ) else {
            return dry_run(args);
        };
        c.run_workflow(&wf, args.clone()).await
    }
}
