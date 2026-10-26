//! `cortex.analyzer.run`.

use anyhow::{Context, Result};
use async_trait::async_trait;
use serde_json::Value;

use super::{arg_str, dry_run, Tool};

/// `cortex.analyzer.run` — `{analyzer_id, data_type, data}`.
pub(crate) struct CortexAnalyzer(pub(crate) Option<connectors::CortexClient>);

#[async_trait]
impl Tool for CortexAnalyzer {
    fn name(&self) -> &'static str {
        "cortex.analyzer.run"
    }
    async fn execute(&self, args: &Value) -> Result<Value> {
        let Some(c) = &self.0 else {
            return dry_run(args);
        };
        let analyzer_id = arg_str(args, &["analyzer_id"]).context("missing analyzer_id")?;
        let data_type = arg_str(args, &["data_type"]).context("missing data_type")?;
        let data = arg_str(args, &["data"]).context("missing data")?;
        c.run_analyzer(analyzer_id, data_type, data).await
    }
}
