//! Optional pull-based ingestors for sources without webhook support.
//!
//! Both pollers are skeletons gated behind env flags. They poll on an
//! interval, convert each record into an [`Envelope`], and publish it via the
//! shared [`NatsSink`]. The actual API calls are left as TODOs with the
//! required credentials documented.

use std::time::Duration;

use agent_protocol::envelope::{Envelope, EnvelopeKind};
use agent_protocol::SUBJECT_ALERTS;
use tracing::{debug, info, warn};

use agent_core::NatsSink;

/// Poll interval for the Office 365 poller (seconds).
const M365_POLL_INTERVAL_SECS: u64 = 300;
/// Poll interval for the Entra sign-in poller (seconds).
const ENTRA_POLL_INTERVAL_SECS: u64 = 300;

/// Polls the Office 365 Unified Audit Log for new audit records.
///
/// Required env credentials:
/// - `M365_TENANT_ID`     — Entra tenant GUID.
/// - `M365_CLIENT_ID`     — app registration client ID with
///   `ActivityFeed.Read` (Office 365 Management APIs).
/// - `M365_CLIENT_SECRET` — app registration client secret.
///
/// TODO: implement the full flow:
/// 1. Acquire an OAuth token from `login.microsoftonline.com/{tenant}/oauth2/v2.0/token`.
/// 2. List available content blobs via `GET /api/v1.0/{tenant}/activity/feed/subscriptions/content`.
/// 3. Fetch each `contentUri`, extract `records`, and track a high-water mark
///    (last `creationTime`) to avoid re-ingestion.
/// 4. Return the records so they can be wrapped in envelopes below.
pub async fn m365_ual_poller(sink: NatsSink) {
    let interval = Duration::from_secs(
        std::env::var("M365_POLL_INTERVAL_SECS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(M365_POLL_INTERVAL_SECS),
    );
    info!(?interval, "m365 UAL poller started");
    loop {
        tokio::time::sleep(interval).await;
        match fetch_m365_records().await {
            Ok(records) => {
                for record in records {
                    let env = Envelope::new("m365-ual", EnvelopeKind::Alert, record);
                    if let Err(e) = sink.publish(SUBJECT_ALERTS, &env).await {
                        warn!(error = %e, "m365 record publish failed");
                    }
                }
            }
            Err(e) => debug!(error = %e, "m365 poll cycle skipped"),
        }
    }
}

/// Polls Microsoft Graph for Entra ID sign-in events.
///
/// Required env credentials:
/// - `ENTRA_TENANT_ID`     — Entra tenant GUID.
/// - `ENTRA_CLIENT_ID`     — app registration client ID with
///   `AuditLog.Read.All` + `Directory.Read.All` (application permissions).
/// - `ENTRA_CLIENT_SECRET` — app registration client secret.
///
/// TODO: implement the full flow:
/// 1. Acquire an OAuth token from `login.microsoftonline.com/{tenant}/oauth2/v2.0/token`.
/// 2. Query `GET https://graph.microsoft.com/v1.0/auditLogs/signIns` filtered
///    on `createdDateTime ge {last_watermark}` and `$top=100`.
/// 3. Persist the watermark (`@odata.nextLink` paging handled per cycle).
/// 4. Return the sign-in records for enveloping below.
pub async fn entra_signin_poller(sink: NatsSink) {
    let interval = Duration::from_secs(
        std::env::var("ENTRA_POLL_INTERVAL_SECS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(ENTRA_POLL_INTERVAL_SECS),
    );
    info!(?interval, "entra sign-in poller started");
    loop {
        tokio::time::sleep(interval).await;
        match fetch_entra_signins().await {
            Ok(records) => {
                for record in records {
                    let env = Envelope::new("entra-signin", EnvelopeKind::Alert, record);
                    if let Err(e) = sink.publish(SUBJECT_ALERTS, &env).await {
                        warn!(error = %e, "entra record publish failed");
                    }
                }
            }
            Err(e) => debug!(error = %e, "entra poll cycle skipped"),
        }
    }
}

/// Fetches new UAL records. Stub: returns an empty list until credentials and
/// the Management API client are wired up.
async fn fetch_m365_records() -> anyhow::Result<Vec<serde_json::Value>> {
    if std::env::var("M365_CLIENT_SECRET").is_err() {
        anyhow::bail!("M365_CLIENT_SECRET not set; poller is a skeleton");
    }
    // TODO: OAuth token + content blob fetch + watermark tracking.
    Ok(Vec::new())
}

/// Fetches new Entra sign-in records. Stub: returns an empty list until
/// credentials and the Graph client are wired up.
async fn fetch_entra_signins() -> anyhow::Result<Vec<serde_json::Value>> {
    if std::env::var("ENTRA_CLIENT_SECRET").is_err() {
        anyhow::bail!("ENTRA_CLIENT_SECRET not set; poller is a skeleton");
    }
    // TODO: OAuth token + /auditLogs/signIns query + watermark tracking.
    Ok(Vec::new())
}
