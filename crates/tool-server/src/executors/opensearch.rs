//! `opensearch.query`.

use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};

use super::{arg_str, dry_run, Tool};
use connectors::OpensearchClient;

/// `index` args must match `OPENSEARCH_INDEX_ALLOWLIST` (comma-separated
/// prefixes ending in `*`, or exact names; default `wazuh-*`). Separators,
/// `..` and query chars are always rejected.
fn index_allowed(name: &str) -> bool {
    if name.is_empty()
        || name.len() > 128
        || name.contains(['/', '?', '#', '&', '=', ' '])
        || name.contains("..")
    {
        return false;
    }
    let spec =
        super::env("OPENSEARCH_INDEX_ALLOWLIST").unwrap_or_else(|| "wazuh-*".to_string());
    spec.split(',')
        .map(str::trim)
        .filter(|e| !e.is_empty())
        .any(|pat| {
            if let Some(prefix) = pat.strip_suffix('*') {
                name.starts_with(prefix)
            } else {
                name == pat
            }
        })
}

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
        let index = arg_str(args, &["index"]).unwrap_or("wazuh-alerts-*");
        anyhow::ensure!(
            index_allowed(index),
            "index `{index}` outside OPENSEARCH_INDEX_ALLOWLIST"
        );
        let q = arg_str(args, &["q"]).unwrap_or("");
        let body = args
            .get("body")
            .cloned()
            .unwrap_or_else(|| json!({ "query": { "match_all": {} } }));
        c.query(index, q, body).await
    }
}
