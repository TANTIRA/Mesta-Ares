//! Human approval queue for `autonomy: approval` tools.
//!
//! A call that policy flags `ApprovalRequired` is parked here instead of
//! being dropped. `POST /approvals/{id}/approve` re-runs the policy engine via
//! [`PolicyEngine::evaluate_approved`] — approval satisfies the autonomy gate
//! only, so the kill switch, args schema, protected assets and blast radius
//! checks still apply — then executes the stored call. `reject` records the
//! human's refusal. Every decision is audited with the approver as actor.
//!
//! Pending approvals live in memory: a restart forgets them, which fails
//! closed (a forgotten approval never executes). Persist to Postgres if you
//! need approvals to survive restarts.

use std::sync::Arc;

use agent_protocol::tool::{Decision, DecisionKind, ToolCall, ToolResult};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use chrono::{DateTime, Duration, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use tracing::{error, info};
use uuid::Uuid;

use crate::AppState;

/// How long an approval stays actionable.
const EXPIRY: Duration = Duration::hours(24);

/// Lifecycle of an approval record.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    /// Waiting on a human.
    Pending,
    /// A human approved and the call was executed.
    Approved,
    /// A human refused the call.
    Rejected,
    /// Not decided within [`EXPIRY`].
    Expired,
}

/// One parked call awaiting a human decision.
#[derive(Debug, Clone, Serialize)]
pub struct Approval {
    /// Approval id, returned on the original `POST /invoke`.
    pub id: Uuid,
    /// The call that was parked.
    pub call: ToolCall,
    /// Why policy required approval.
    pub reason: String,
    /// Current lifecycle state.
    pub status: Status,
    /// When the call was parked.
    pub created_at: DateTime<Utc>,
    /// When the decision was made.
    pub decided_at: Option<DateTime<Utc>>,
    /// Who decided.
    pub decided_by: Option<String>,
    /// Execution result (approve only).
    pub result: Option<ToolResult>,
}

/// In-memory approval queue.
pub struct Store {
    items: DashMap<Uuid, Approval>,
}

impl Store {
    /// Empty queue.
    pub fn new() -> Self {
        Self {
            items: DashMap::new(),
        }
    }

    /// Parks `call` and returns its approval id.
    pub fn insert(&self, call: &ToolCall, reason: &str) -> Uuid {
        let id = Uuid::new_v4();
        self.items.insert(
            id,
            Approval {
                id,
                call: call.clone(),
                reason: reason.to_string(),
                status: Status::Pending,
                created_at: Utc::now(),
                decided_at: None,
                decided_by: None,
                result: None,
            },
        );
        id
    }

    /// One record, expired-on-read when stale.
    pub fn get(&self, id: &Uuid) -> Option<Approval> {
        let mut rec = self.items.get_mut(id)?;
        expire(&mut rec);
        Some(rec.clone())
    }

    /// Records, newest first, optionally filtered by status.
    pub fn list(&self, status: Option<Status>) -> Vec<Approval> {
        let mut out: Vec<Approval> = self
            .items
            .iter_mut()
            .map(|mut r| {
                expire(&mut r);
                r.clone()
            })
            .filter(|a| status.is_none_or(|s| a.status == s))
            .collect();
        out.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        out
    }

    /// Atomically moves a pending record to `status`. Fails when the record
    /// is absent, already decided, or past [`EXPIRY`] — that check is the
    /// double-execution guard.
    pub fn settle(&self, id: &Uuid, status: Status, by: &str) -> Result<Approval, SettleError> {
        let mut rec = self.items.get_mut(id).ok_or(SettleError::NotFound)?;
        expire(&mut rec);
        if rec.status != Status::Pending {
            return Err(SettleError::AlreadyDecided(rec.status));
        }
        rec.status = status;
        rec.decided_at = Some(Utc::now());
        rec.decided_by = Some(by.to_string());
        Ok(rec.clone())
    }

    /// Attaches the execution result to a decided record.
    pub fn attach_result(&self, id: &Uuid, result: ToolResult) {
        if let Some(mut rec) = self.items.get_mut(id) {
            rec.result = Some(result);
        }
    }
}

impl Default for Store {
    fn default() -> Self {
        Self::new()
    }
}

/// Marks a pending record expired once [`EXPIRY`] has passed.
fn expire(rec: &mut Approval) {
    if rec.status == Status::Pending && Utc::now() - rec.created_at > EXPIRY {
        rec.status = Status::Expired;
    }
}

/// Why a settle failed.
#[derive(Debug, PartialEq, Eq)]
pub enum SettleError {
    /// No record with that id.
    NotFound,
    /// Already approved, rejected or expired.
    AlreadyDecided(Status),
}

/// Query params for `GET /approvals`.
#[derive(Debug, Deserialize)]
pub struct ListQuery {
    /// Filter by status (`pending`, `approved`, `rejected`, `expired`).
    status: Option<Status>,
}

/// `GET /approvals` — queue contents, newest first.
pub async fn list(
    State(s): State<Arc<AppState>>,
    Query(q): Query<ListQuery>,
) -> Json<Vec<Approval>> {
    Json(s.approvals.list(q.status))
}

/// Body for approve/reject.
#[derive(Debug, Deserialize)]
pub struct DecideBody {
    /// Identity of the human deciding; defaults to `operator`.
    #[serde(default)]
    approver: Option<String>,
    /// Optional note (reject reason).
    #[serde(default)]
    reason: Option<String>,
}

/// Approve/reject response: the record plus the fresh policy decision and
/// execution result (both absent on reject).
#[derive(Debug, Serialize)]
pub struct Outcome {
    /// The settled record.
    approval: Approval,
    /// Decision from re-evaluating the stored call (approve only).
    #[serde(skip_serializing_if = "Option::is_none")]
    decision: Option<Decision>,
    /// Execution result (approve only).
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<ToolResult>,
}

/// `POST /approvals/{id}/approve` — re-check policy, then execute.
pub async fn approve(
    State(s): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    body: Option<Json<DecideBody>>,
) -> Result<Json<Outcome>, (StatusCode, String)> {
    let by = body
        .as_ref()
        .and_then(|Json(b)| b.approver.clone())
        .unwrap_or_else(|| "operator".to_string());

    let settled = s
        .approvals
        .settle(&id, Status::Approved, &by)
        .map_err(|e| match e {
            SettleError::NotFound => (StatusCode::NOT_FOUND, format!("approval not found: {id}")),
            SettleError::AlreadyDecided(st) => (
                StatusCode::CONFLICT,
                format!("approval {id} already decided: {st:?}"),
            ),
        })?;
    let call = settled.call.clone();

    let decision = {
        let mut ctx = s.eval_ctx.lock().await;
        s.engine.evaluate_approved(&call, &mut ctx).await
    };

    let result = match &decision.kind {
        DecisionKind::Allow => crate::execute(&s, &call).await,
        _ => ToolResult {
            call_id: call.id,
            ok: false,
            output: serde_json::Value::Null,
            error: Some(format!("denied after approval: {}", decision.reason)),
        },
    };
    s.approvals.attach_result(&id, result.clone());

    if let Some(writer) = &s.audit {
        if let Err(e) = writer
            .append(
                &by,
                &call.tool,
                &call.args,
                decision.kind,
                &format!("approved by {by}: {}", decision.reason),
                call.correlation_id,
            )
            .await
        {
            error!(error = %e, "audit append failed");
        }
    }
    info!(approval = %id, tool = %call.tool, approver = %by, "approval executed");

    Ok(Json(Outcome {
        approval: s.approvals.get(&id).unwrap_or(settled),
        decision: Some(decision),
        result: Some(result),
    }))
}

/// `POST /approvals/{id}/reject` — record the refusal, never execute.
pub async fn reject(
    State(s): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    body: Option<Json<DecideBody>>,
) -> Result<Json<Outcome>, (StatusCode, String)> {
    let by = body
        .as_ref()
        .and_then(|Json(b)| b.approver.clone())
        .unwrap_or_else(|| "operator".to_string());
    let note = body.as_ref().and_then(|Json(b)| b.reason.clone());

    let settled = s
        .approvals
        .settle(&id, Status::Rejected, &by)
        .map_err(|e| match e {
            SettleError::NotFound => (StatusCode::NOT_FOUND, format!("approval not found: {id}")),
            SettleError::AlreadyDecided(st) => (
                StatusCode::CONFLICT,
                format!("approval {id} already decided: {st:?}"),
            ),
        })?;
    let call = settled.call.clone();

    if let Some(writer) = &s.audit {
        let reason = match &note {
            Some(n) => format!("rejected by {by}: {n}"),
            None => format!("rejected by {by}"),
        };
        if let Err(e) = writer
            .append(
                &by,
                &call.tool,
                &call.args,
                DecisionKind::Deny,
                &reason,
                call.correlation_id,
            )
            .await
        {
            error!(error = %e, "audit append failed");
        }
    }
    info!(approval = %id, tool = %call.tool, approver = %by, "approval rejected");

    Ok(Json(Outcome {
        approval: settled,
        decision: None,
        result: None,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn call() -> ToolCall {
        ToolCall {
            id: Uuid::new_v4(),
            tool: "misp.sighting.add".to_string(),
            args: json!({"value": "203.0.113.9"}),
            actor: "response-agent".to_string(),
            idempotency_key: String::new(),
            correlation_id: Uuid::new_v4(),
        }
    }

    #[test]
    fn insert_parks_pending_and_lists_newest_first() {
        let store = Store::new();
        let a = store.insert(&call(), "needs approval");
        let b = store.insert(&call(), "needs approval");
        let all = store.list(None);
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].id, b, "newest first");
        assert!(all.iter().all(|r| r.status == Status::Pending));
        assert_eq!(store.list(Some(Status::Pending)).len(), 2);
        assert_eq!(store.list(Some(Status::Approved)).len(), 0);
        assert!(store.get(&a).is_some());
    }

    #[test]
    fn settle_is_single_shot() {
        let store = Store::new();
        let id = store.insert(&call(), "needs approval");
        let ok = store.settle(&id, Status::Approved, "alice").unwrap();
        assert_eq!(ok.status, Status::Approved);
        assert_eq!(ok.decided_by.as_deref(), Some("alice"));
        assert_eq!(
            store.settle(&id, Status::Approved, "bob").unwrap_err(),
            SettleError::AlreadyDecided(Status::Approved)
        );
        assert_eq!(
            store.settle(&Uuid::new_v4(), Status::Rejected, "bob").unwrap_err(),
            SettleError::NotFound
        );
    }

    #[test]
    fn reject_then_approve_is_conflict() {
        let store = Store::new();
        let id = store.insert(&call(), "needs approval");
        store.settle(&id, Status::Rejected, "alice").unwrap();
        assert_eq!(
            store.settle(&id, Status::Approved, "bob").unwrap_err(),
            SettleError::AlreadyDecided(Status::Rejected)
        );
    }

    #[test]
    fn stale_pending_records_expire() {
        let store = Store::new();
        let id = store.insert(&call(), "needs approval");
        {
            let mut rec = store.items.get_mut(&id).unwrap();
            rec.created_at = Utc::now() - EXPIRY - Duration::minutes(1);
        }
        assert_eq!(store.get(&id).unwrap().status, Status::Expired);
        assert_eq!(
            store.settle(&id, Status::Approved, "alice").unwrap_err(),
            SettleError::AlreadyDecided(Status::Expired)
        );
    }

    #[test]
    fn result_attaches_to_decided_record() {
        let store = Store::new();
        let id = store.insert(&call(), "needs approval");
        store.settle(&id, Status::Approved, "alice").unwrap();
        let res = ToolResult {
            call_id: Uuid::new_v4(),
            ok: true,
            output: json!({"saved": true}),
            error: None,
        };
        store.attach_result(&id, res);
        assert!(store.get(&id).unwrap().result.unwrap().ok);
    }
}
