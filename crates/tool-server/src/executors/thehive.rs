//! TheHive case and observable executors.

use anyhow::{Context, Result};
use async_trait::async_trait;
use serde_json::Value;

use super::{arg_str, dry_run, Tool};
use connectors::{NewCase, NewObservable, ThehiveClient};

/// `thehive.case.create`.
pub(crate) struct ThehiveCase(pub(crate) Option<ThehiveClient>);

#[async_trait]
impl Tool for ThehiveCase {
    fn name(&self) -> &'static str {
        "thehive.case.create"
    }
    async fn execute(&self, args: &Value) -> Result<Value> {
        let Some(c) = &self.0 else {
            return dry_run(args);
        };
        let case: NewCase = serde_json::from_value(args.clone()).context("invalid NewCase args")?;
        Ok(serde_json::to_value(c.create_case(&case).await?)?)
    }
}

/// `thehive.case.update` — `{case_id, patch}`.
pub(crate) struct ThehiveCaseUpdate(pub(crate) Option<ThehiveClient>);

#[async_trait]
impl Tool for ThehiveCaseUpdate {
    fn name(&self) -> &'static str {
        "thehive.case.update"
    }
    async fn execute(&self, args: &Value) -> Result<Value> {
        let Some(c) = &self.0 else {
            return dry_run(args);
        };
        let case_id = arg_str(args, &["case_id"]).context("missing case_id")?;
        let patch = args.get("patch").cloned().unwrap_or_else(|| args.clone());
        Ok(serde_json::to_value(c.update_case(case_id, &patch).await?)?)
    }
}

/// `thehive.observable.add` — `{case_id, observable:{...}}` or flat fields.
pub(crate) struct ThehiveObservable(pub(crate) Option<ThehiveClient>);

#[async_trait]
impl Tool for ThehiveObservable {
    fn name(&self) -> &'static str {
        "thehive.observable.add"
    }
    async fn execute(&self, args: &Value) -> Result<Value> {
        let Some(c) = &self.0 else {
            return dry_run(args);
        };
        let case_id = arg_str(args, &["case_id"]).context("missing case_id")?;
        let obs_val = args
            .get("observable")
            .cloned()
            .unwrap_or_else(|| args.clone());
        let obs: NewObservable =
            serde_json::from_value(obs_val).context("invalid NewObservable args")?;
        Ok(serde_json::to_value(
            c.add_observable(case_id, &obs).await?,
        )?)
    }
}
