//! `opensearch.query`.

use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};

use super::{arg_str, dry_run, Tool};
use connectors::OpensearchClient;

/// `opensearch.query` — runs a `_search` through [`OpensearchClient`].
pub(crate) struct OpensearchQuery(pub(crate) Option<OpensearchClient>);

#[async_trait]
impl Tool for OpensearchQuery {
    fn name(&self) -> &'static str {
        "opensearch.query"
    }
    async fn execute(&self, args: &Value) -> Result<Value> {
        let Some(c) = &self.0 else {
            return dry_run(args);
        };
        let index = arg_str(args, &["index"]).unwrap_or("_all");
        let q = arg_str(args, &["q"]).unwrap_or("");
        let body = args
            .get("body")
            .cloned()
            .unwrap_or_else(|| json!({ "query": { "match_all": {} } }));
        c.query(index, q, body).await
    }
}
