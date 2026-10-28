//! TheHive case and observable client.
use std::collections::HashMap;

use anyhow::{Context, Result};

use reqwest::Client;

use serde::{Deserialize, Serialize};

use serde_json::Value;

use crate::http::{join, json_ok, segment, send_with_retry, Auth};

// ---------------------------------------------------------------------------
// TheHive
// ---------------------------------------------------------------------------

/// New case payload for TheHive (`POST /api/v1/case`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewCase {
    pub title: String,
    pub description: String,
    #[serde(default)]
    pub severity: u8,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub tlp: u8,
}

/// A case as returned by TheHive; unknown fields are preserved in `extra`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Case {
    #[serde(rename = "_id")]
    pub id: String,
    #[serde(default)]
    pub title: String,
    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
}

/// New observable payload (`POST /api/v1/case/{id}/observable`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewObservable {
    #[serde(rename = "dataType")]
    pub data_type: String,
    pub data: String,
    #[serde(default)]
    pub message: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub tlp: Option<u8>,
    #[serde(default)]
    pub ioc: bool,
}

/// An observable as returned by TheHive.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Observable {
    #[serde(rename = "_id")]
    pub id: String,
    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
}

/// TheHive case-management client.
#[derive(Clone)]
pub struct ThehiveClient {
    base: String,
    http: Client,
    auth: Auth,
}

impl ThehiveClient {
    pub fn new(base_url: impl Into<String>, auth: Auth) -> Self {
        Self {
            base: base_url.into().trim_end_matches('/').to_string(),
            http: Client::new(),
            auth,
        }
    }

    /// `POST /api/v1/case`.
    /// `POST /api/v1/query` — TheHive's search DSL; `query` is one filter.
    pub async fn query(&self, query: Value) -> Result<Value> {
        let req = self
            .auth
            .apply(self.http.post(join(&self.base, "/api/v1/query")))
            .json(&serde_json::json!({ "query": [query] }));
        json_ok(send_with_retry(req).await.context("thehive query")?).await
    }

    pub async fn create_case(&self, case: &NewCase) -> Result<Case> {
        let req = self
            .auth
            .apply(self.http.post(join(&self.base, "/api/v1/case")))
            .json(case);
        json_ok(send_with_retry(req).await.context("thehive create_case")?).await
    }

    /// `POST /api/v1/case/{case_id}/observable`.
    pub async fn add_observable(
        &self,
        case_id: &str,
        observable: &NewObservable,
    ) -> Result<Observable> {
        let case_id = segment(case_id, "case_id")?;
        let req = self
            .auth
            .apply(self.http.post(join(
                &self.base,
                &format!("/api/v1/case/{case_id}/observable"),
            )))
            .json(observable);
        json_ok(
            send_with_retry(req)
                .await
                .context("thehive add_observable")?,
        )
        .await
    }

    /// `PATCH /api/v1/case/{case_id}` with an arbitrary JSON patch body.
    pub async fn update_case(&self, case_id: &str, patch: &Value) -> Result<Case> {
        let case_id = segment(case_id, "case_id")?;
        let req = self
            .auth
            .apply(
                self.http
                    .patch(join(&self.base, &format!("/api/v1/case/{case_id}"))),
            )
            .json(patch);
        json_ok(send_with_retry(req).await.context("thehive update_case")?).await
    }
}
