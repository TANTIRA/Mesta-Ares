//! OpenSearch search/index client.
use anyhow::{Context, Result};

use reqwest::Client;

use serde_json::Value;

use crate::http::{join, json_ok, send_with_retry, Auth};

// ---------------------------------------------------------------------------
// OpenSearch
// ---------------------------------------------------------------------------

/// OpenSearch / Elasticsearch `_search` client.
#[derive(Clone)]
pub struct OpensearchClient {
    base: String,
    http: Client,
    auth: Auth,
}

impl OpensearchClient {
    pub fn new(base_url: impl Into<String>, auth: Auth) -> Self {
        Self {
            base: base_url.into().trim_end_matches('/').to_string(),
            http: Client::new(),
            auth,
        }
    }

    /// Indexes `doc` into `index` as a new document (`POST /{index}/_doc`).
    pub async fn index_doc(&self, index: &str, doc: &Value) -> Result<Value> {
        let req = self
            .auth
            .apply(self.http.post(join(&self.base, &format!("/{index}/_doc"))))
            .json(doc);
        json_ok(send_with_retry(req).await.context("opensearch index_doc")?).await
    }

    /// `POST {base}/{path}` with a JSON body (monitors, plugins, etc.).
    pub async fn post_json(&self, path: &str, body: &Value) -> Result<Value> {
        let req = self
            .auth
            .apply(self.http.post(join(&self.base, path)))
            .json(body);
        json_ok(send_with_retry(req).await.context("opensearch post")?).await
    }

    /// `POST {base}/{index}/_search` with the given query body. `q` is passed
    /// through as the optional URI `?q=` query-string search parameter.
    pub async fn query(&self, index: &str, q: &str, body: Value) -> Result<Value> {
        let req = self
            .auth
            .apply(
                self.http
                    .post(join(&self.base, &format!("/{index}/_search"))),
            )
            .query(&[("q", q)])
            .json(&body);
        json_ok(send_with_retry(req).await.context("opensearch query")?).await
    }
}
