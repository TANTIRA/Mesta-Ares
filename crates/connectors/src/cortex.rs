//! Cortex analyzer client.
use anyhow::{Context, Result};

use reqwest::Client;

use serde_json::Value;

use crate::http::{join, json_ok, send_with_retry, Auth};

// ---------------------------------------------------------------------------
// Cortex
// ---------------------------------------------------------------------------

/// Cortex analyzer runner.
#[derive(Clone)]
pub struct CortexClient {
    base: String,
    http: Client,
    auth: Auth,
}

impl CortexClient {
    pub fn new(base_url: impl Into<String>, auth: Auth) -> Self {
        Self {
            base: base_url.into().trim_end_matches('/').to_string(),
            http: Client::new(),
            auth,
        }
    }

    /// `POST /api/analyzer/{analyzer_id}/run` — returns the submitted job.
    pub async fn run_analyzer(
        &self,
        analyzer_id: &str,
        data_type: &str,
        data: &str,
    ) -> Result<Value> {
        let body = serde_json::json!({
            "data": data,
            "dataType": data_type,
            "tlp": 2,
        });
        let req = self
            .auth
            .apply(self.http.post(join(
                &self.base,
                &format!("/api/analyzer/{analyzer_id}/run"),
            )))
            .json(&body);
        json_ok(send_with_retry(req).await.context("cortex run_analyzer")?).await
    }
}
