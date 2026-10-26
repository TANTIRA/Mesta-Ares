//! Shuffle workflow client.
use anyhow::{Context, Result};

use reqwest::Client;

use serde_json::Value;

use crate::http::{join, json_ok, send_with_retry, Auth};

// ---------------------------------------------------------------------------
// Shuffle (SOAR workflows)
// ---------------------------------------------------------------------------

/// Shuffle workflow executor.
#[derive(Clone)]
pub struct ShuffleClient {
    base: String,
    http: Client,
    auth: Auth,
}

impl ShuffleClient {
    pub fn new(base_url: impl Into<String>, auth: Auth) -> Self {
        Self {
            base: base_url.into().trim_end_matches('/').to_string(),
            http: Client::new(),
            auth,
        }
    }

    /// `POST /api/v1/workflows/{workflow_id}/execute` — body passed through.
    pub async fn run_workflow(&self, workflow_id: &str, body: Value) -> Result<Value> {
        let req = self
            .auth
            .apply(self.http.post(join(
                &self.base,
                &format!("/api/v1/workflows/{workflow_id}/execute"),
            )))
            .json(&body);
        json_ok(send_with_retry(req).await.context("shuffle run_workflow")?).await
    }
}
