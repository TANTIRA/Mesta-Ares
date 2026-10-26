//! MISP search, attribute, warninglist and sighting executors.

use anyhow::{Context, Result};
use async_trait::async_trait;
use serde_json::Value;

use super::{arg_str, dry_run, env, Tool};
use connectors::{MispClient, NewAttribute};

/// `misp.search` — `{value}`.
pub(crate) struct MispSearch(pub(crate) Option<MispClient>);

#[async_trait]
impl Tool for MispSearch {
    fn name(&self) -> &'static str {
        "misp.search"
    }
    async fn execute(&self, args: &Value) -> Result<Value> {
        let Some(c) = &self.0 else {
            return dry_run(args);
        };
        let value = arg_str(args, &["value"]).context("missing value")?;
        Ok(serde_json::to_value(c.search(value).await?)?)
    }
}

/// `misp.attribute.add` and `misp.ioc.block` — `{event_id?, attribute|...}`.
/// `event_id` falls back to `MISP_BLOCK_EVENT_ID`.
pub(crate) struct MispAttribute {
    pub(crate) name: &'static str,
    pub(crate) client: Option<MispClient>,
}

#[async_trait]
impl Tool for MispAttribute {
    fn name(&self) -> &'static str {
        self.name
    }
    async fn execute(&self, args: &Value) -> Result<Value> {
        let Some(c) = &self.client else {
            return dry_run(args);
        };
        let event_id = arg_str(args, &["event_id"])
            .map(str::to_string)
            .or_else(|| env("MISP_BLOCK_EVENT_ID"))
            .context("missing event_id (and MISP_BLOCK_EVENT_ID unset)")?;
        let attr_val = args
            .get("attribute")
            .cloned()
            .unwrap_or_else(|| args.clone());
        let attr: NewAttribute =
            serde_json::from_value(attr_val).context("invalid NewAttribute args")?;
        c.add_attribute(&event_id, &attr).await
    }
}

/// `misp.attribute.search` — `{value, limit?}`.
pub(crate) struct MispAttributeSearch(pub(crate) Option<MispClient>);

#[async_trait]
impl Tool for MispAttributeSearch {
    fn name(&self) -> &'static str {
        "misp.attribute.search"
    }
    async fn execute(&self, args: &Value) -> Result<Value> {
        let Some(c) = &self.0 else {
            return dry_run(args);
        };
        let value = arg_str(args, &["value"]).context("missing value")?;
        let limit = args.get("limit").and_then(Value::as_u64).unwrap_or(50) as u32;
        Ok(serde_json::to_value(
            c.search_attributes(value, limit).await?,
        )?)
    }
}

/// `misp.event.view` — `{event_id}`.
pub(crate) struct MispEventView(pub(crate) Option<MispClient>);

#[async_trait]
impl Tool for MispEventView {
    fn name(&self) -> &'static str {
        "misp.event.view"
    }
    async fn execute(&self, args: &Value) -> Result<Value> {
        let Some(c) = &self.0 else {
            return dry_run(args);
        };
        let event_id = arg_str(args, &["event_id"]).context("missing event_id")?;
        c.event(event_id).await
    }
}

/// `misp.warninglist.check` — `{values: [..]}` or `{value}`.
pub(crate) struct MispWarninglist(pub(crate) Option<MispClient>);

#[async_trait]
impl Tool for MispWarninglist {
    fn name(&self) -> &'static str {
        "misp.warninglist.check"
    }
    async fn execute(&self, args: &Value) -> Result<Value> {
        let Some(c) = &self.0 else {
            return dry_run(args);
        };
        let values: Vec<String> = match args.get("values").and_then(Value::as_array) {
            Some(arr) => arr
                .iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect(),
            None => vec![arg_str(args, &["value"])
                .context("missing values")?
                .to_string()],
        };
        Ok(serde_json::json!({ "matched": c.check_warninglists(&values).await? }))
    }
}

/// `misp.sighting.add` — `{values: [..], source?}`.
pub(crate) struct MispSighting(pub(crate) Option<MispClient>);

#[async_trait]
impl Tool for MispSighting {
    fn name(&self) -> &'static str {
        "misp.sighting.add"
    }
    async fn execute(&self, args: &Value) -> Result<Value> {
        let Some(c) = &self.0 else {
            return dry_run(args);
        };
        let values: Vec<String> = match args.get("values").and_then(Value::as_array) {
            Some(arr) => arr
                .iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect(),
            None => vec![arg_str(args, &["value"])
                .context("missing values")?
                .to_string()],
        };
        let source = arg_str(args, &["source"])
            .map(str::to_string)
            .or_else(|| env("MISP_SIGHTING_SOURCE"));
        c.add_sightings(&values, source.as_deref()).await
    }
}
