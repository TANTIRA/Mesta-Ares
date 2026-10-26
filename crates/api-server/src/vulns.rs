//! Vulnerability-intelligence endpoints backed by the NVD API 2.0 client
//! (`GET /vulns`, `/vulns/:cve_id`, `/vulns/:cve_id/history`) plus the local
//! CVSS scoring engine (`POST /cvss/score`).

use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::AppState;

/// Cheap shape check before hitting NVD.
fn valid_cve_id(id: &str) -> bool {
    let id = id.as_bytes();
    let ok = id.len() >= 13
        && id.starts_with(b"CVE-")
        && id[4..8].iter().all(u8::is_ascii_digit)
        && id[8] == b'-'
        && id[9..].iter().all(u8::is_ascii_digit);
    ok && id.len() <= 16
}

/// Query params for `GET /vulns`.
#[derive(Debug, Deserialize)]
pub struct VulnsQuery {
    /// Look-back window in days (default 1, cap 120 — the NVD API limit).
    days: Option<u32>,
    /// Keyword filter (`keywordSearch`).
    keyword: Option<String>,
    /// `cvssV4Severity` filter (LOW/MEDIUM/HIGH/CRITICAL).
    cvss_v4_severity: Option<String>,
    /// `cvssV3Severity` filter.
    cvss_v3_severity: Option<String>,
    /// Restrict to CISA KEV-listed CVEs.
    has_kev: Option<bool>,
    /// Pagination offset.
    start_index: Option<u64>,
}

/// `GET /vulns` — CVEs published in the last `days` window, server-paged.
pub async fn list_vulns(
    State(s): State<Arc<AppState>>,
    Query(q): Query<VulnsQuery>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let days = q.days.unwrap_or(1).clamp(1, 120);
    let to = chrono::Utc::now();
    let from = to - chrono::Duration::days(days as i64);
    let fmt = |t: chrono::DateTime<chrono::Utc>| t.format("%Y-%m-%dT%H:%M:%S%.3f").to_string();

    let mut extra: Vec<(&str, String)> = Vec::new();
    if let Some(kw) = q.keyword.filter(|k| !k.is_empty()) {
        extra.push(("keywordSearch", kw));
    }
    if let Some(sev) = q.cvss_v4_severity.filter(|v| !v.is_empty()) {
        extra.push(("cvssV4Severity", sev.to_uppercase()));
    }
    if let Some(sev) = q.cvss_v3_severity.filter(|v| !v.is_empty()) {
        extra.push(("cvssV3Severity", sev.to_uppercase()));
    }
    if q.has_kev.unwrap_or(false) {
        extra.push(("hasKev", String::new()));
    }

    s.nvd
        .list_published(&fmt(from), &fmt(to), q.start_index.unwrap_or(0), &extra)
        .await
        .map(|p| Json(serde_json::to_value(p).unwrap_or(Value::Null)))
        .map_err(|e| (StatusCode::BAD_GATEWAY, e.to_string()))
}

/// `GET /vulns/:cve_id` — one CVE record (Redis-cached through the enricher).
pub async fn get_vuln(
    State(s): State<Arc<AppState>>,
    Path(cve_id): Path<String>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let cve_id = cve_id.to_uppercase();
    if !valid_cve_id(&cve_id) {
        return Err((StatusCode::BAD_REQUEST, "malformed CVE id".to_string()));
    }
    match s.enricher.enrich_cve(&cve_id).await {
        Some(rec) => Ok(Json(serde_json::to_value(rec).unwrap_or(Value::Null))),
        None => Err((
            StatusCode::NOT_FOUND,
            format!("{cve_id}: no NVD record (unknown, reserved, or rejected)"),
        )),
    }
}

/// `GET /vulns/:cve_id/history` — raw NVD change-history events.
pub async fn get_vuln_history(
    State(s): State<Arc<AppState>>,
    Path(cve_id): Path<String>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let cve_id = cve_id.to_uppercase();
    if !valid_cve_id(&cve_id) {
        return Err((StatusCode::BAD_REQUEST, "malformed CVE id".to_string()));
    }
    s.nvd
        .get_history(&cve_id)
        .await
        .map(Json)
        .map_err(|e| (StatusCode::BAD_GATEWAY, e.to_string()))
}

/// `POST /cvss/score` — score a v3.1/v4.0 vector locally.
pub async fn score_cvss(
    State(_s): State<Arc<AppState>>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let vector = body
        .get("vector")
        .and_then(Value::as_str)
        .ok_or((StatusCode::BAD_REQUEST, "missing `vector`".to_string()))?;
    cvss::parse_vector(vector)
        .map(|sc| Json(serde_json::to_value(sc).unwrap_or(Value::Null)))
        .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))
}

/// `GET /vulns/advisories` — vuln-agent advisories persisted to Postgres.
#[derive(Debug, Deserialize)]
pub struct AdvisoryQuery {
    limit: Option<i64>,
}

pub async fn list_advisories(
    State(s): State<Arc<AppState>>,
    Query(q): Query<AdvisoryQuery>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let limit = q.limit.unwrap_or(100).min(1000);
    let rows: Vec<Value> = sqlx::query_scalar(
        "SELECT to_jsonb(t) FROM (
             SELECT cve_id, first_seen, last_checked, severity, score, kev,
                    summary, watchlist_match, case_id, status
             FROM vuln_advisories ORDER BY first_seen DESC LIMIT $1
         ) t",
    )
    .bind(limit)
    .fetch_all(&s.pool)
    .await
    .map_err(|e| (StatusCode::BAD_GATEWAY, e.to_string()))?;
    Ok(Json(json!({"advisories": rows})))
}

#[cfg(test)]
mod tests {
    use super::valid_cve_id;

    #[test]
    fn valid_cve_id_accepts_real_ids() {
        assert!(valid_cve_id("CVE-2024-1234"));
        assert!(valid_cve_id("CVE-2024-1234567"));
    }

    #[test]
    fn valid_cve_id_rejects_malformed_ids() {
        for bad in [
            "cve-2024-1234",
            "CVE-24-1234",
            "CVE-2024-",
            "CVE-2024-12a4",
            "CVE-2024-1234 ",
            "CVE-2024-12345678",
            "",
        ] {
            assert!(!valid_cve_id(bad), "{bad} must be rejected");
        }
    }
}
