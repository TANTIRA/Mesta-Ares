//! Microsoft Graph and Slack notification executors.

use anyhow::{Context, Result};
use async_trait::async_trait;
use serde_json::{json, Value};

use super::{arg_str, dry_run, env, http, Tool};

/// `entra.user.disable` / `entra.user.revoke_sessions` via Microsoft Graph.
/// Real call requires `ENTRA_BEARER_TOKEN`; otherwise dry-run.
pub(crate) struct EntraUser {
    pub(crate) name: &'static str,
}

#[async_trait]
impl Tool for EntraUser {
    fn name(&self) -> &'static str {
        self.name
    }
    async fn execute(&self, args: &Value) -> Result<Value> {
        let Some(token) = env("ENTRA_BEARER_TOKEN").or_else(|| env("GRAPH_BEARER_TOKEN")) else {
            return dry_run(args);
        };
        let user_id = arg_str(args, &["user_id"]).context("missing user_id")?;
        let user_id = connectors::segment(user_id, "user_id")?;
        let req = match self.name {
            "entra.user.disable" => http()
                .patch(format!("https://graph.microsoft.com/v1.0/users/{user_id}"))
                .json(&json!({ "accountEnabled": false })),
            "entra.user.enable" => http()
                .patch(format!("https://graph.microsoft.com/v1.0/users/{user_id}"))
                .json(&json!({ "accountEnabled": true })),
            "entra.user.revoke_sessions" => http().post(format!(
                "https://graph.microsoft.com/v1.0/users/{user_id}/revokeSignInSessions"
            )),
            other => anyhow::bail!("unknown entra executor `{other}`"),
        };
        Ok(req
            .bearer_auth(token)
            .send()
            .await
            .context("graph request")?
            .error_for_status()
            .context("graph HTTP error")?
            .json::<Value>()
            .await
            .unwrap_or(Value::Null))
    }
}

/// `notify.slack.post` — posts to `SLACK_WEBHOOK_URL` if configured.
pub(crate) struct SlackNotify;

#[async_trait]
impl Tool for SlackNotify {
    fn name(&self) -> &'static str {
        "notify.slack.post"
    }
    async fn execute(&self, args: &Value) -> Result<Value> {
        let Some(webhook) = env("SLACK_WEBHOOK_URL") else {
            return dry_run(args);
        };
        let body = if args.get("text").is_some() || args.get("blocks").is_some() {
            args.clone()
        } else {
            json!({ "text": args.get("message").cloned().unwrap_or_else(|| args.clone()) })
        };
        Ok(http()
            .post(webhook)
            .json(&body)
            .send()
            .await
            .context("slack webhook")?
            .error_for_status()
            .context("slack HTTP error")?
            .text()
            .await
            .map(|t| json!({ "response": t }))?)
    }
}
