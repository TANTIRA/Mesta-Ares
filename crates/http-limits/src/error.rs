//! One error envelope for every HTTP service.
//!
//! ```json
//! {
//!   "error": {
//!     "code": "VALIDATION_ERROR",
//!     "message": "Invalid input",
//!     "details": [{ "field": "vector", "message": "unrecognised CVSS prefix" }]
//!   }
//! }
//! ```
//!
//! This crate already owns the shared HTTP edge concerns (`auth`, rate
//! limiting), so the envelope lives here rather than in a new crate: every
//! service that answers HTTP already depends on it.
//!
//! `details` is omitted when empty, so simple failures stay one field shorter
//! while validation failures can name the offending field.

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;

/// Machine-readable failure class, spelled the way the documented contract
/// names it.
pub mod code {
    /// Malformed request the caller can fix.
    pub const BAD_REQUEST: &str = "BAD_REQUEST";
    /// Request parsed but failed validation.
    pub const VALIDATION_ERROR: &str = "VALIDATION_ERROR";
    /// Missing or invalid credentials.
    pub const UNAUTHORIZED: &str = "UNAUTHORIZED";
    /// Resource does not exist.
    pub const NOT_FOUND: &str = "NOT_FOUND";
    /// State transition not allowed (already decided, wrong status, ...).
    pub const CONFLICT: &str = "CONFLICT";
    /// Caller exceeded a rate limit.
    pub const RATE_LIMITED: &str = "RATE_LIMITED";
    /// A dependency (OpenSearch, TheHive, NVD, NATS, Postgres) failed.
    pub const UPSTREAM_ERROR: &str = "UPSTREAM_ERROR";
    /// This service failed internally.
    pub const INTERNAL_ERROR: &str = "INTERNAL_ERROR";
    /// Feature or credential not configured.
    pub const SERVICE_UNAVAILABLE: &str = "SERVICE_UNAVAILABLE";
}

/// One field-level problem inside a [`Detail::details`] list.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Detail {
    /// Offending field, when the failure is field-scoped.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub field: Option<String>,
    /// What is wrong with it.
    pub message: String,
}

/// The `error` object itself.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Body {
    /// Stable code from [`code`].
    pub code: String,
    /// Human-readable summary.
    pub message: String,
    /// Field-level problems; omitted when empty.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub details: Vec<Detail>,
}

/// An error response: status plus the documented envelope.
#[derive(Debug, Clone)]
pub struct ApiError {
    /// HTTP status.
    pub status: StatusCode,
    /// Envelope payload.
    pub body: Body,
}

impl ApiError {
    /// Builds an error from an explicit status and code.
    pub fn new(status: StatusCode, code: &str, message: impl Into<String>) -> Self {
        Self {
            status,
            body: Body {
                code: code.to_string(),
                message: message.into(),
                details: Vec::new(),
            },
        }
    }

    /// `400 BAD_REQUEST`.
    pub fn bad_request(message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, code::BAD_REQUEST, message)
    }

    /// `400 VALIDATION_ERROR`.
    pub fn validation(message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, code::VALIDATION_ERROR, message)
    }

    /// `401 UNAUTHORIZED`.
    pub fn unauthorized(message: impl Into<String>) -> Self {
        Self::new(StatusCode::UNAUTHORIZED, code::UNAUTHORIZED, message)
    }

    /// `404 NOT_FOUND`.
    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new(StatusCode::NOT_FOUND, code::NOT_FOUND, message)
    }

    /// `409 CONFLICT`.
    pub fn conflict(message: impl Into<String>) -> Self {
        Self::new(StatusCode::CONFLICT, code::CONFLICT, message)
    }

    /// `429 RATE_LIMITED`.
    pub fn rate_limited(message: impl Into<String>) -> Self {
        Self::new(StatusCode::TOO_MANY_REQUESTS, code::RATE_LIMITED, message)
    }

    /// `502 UPSTREAM_ERROR`.
    pub fn upstream(message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_GATEWAY, code::UPSTREAM_ERROR, message)
    }

    /// `500 INTERNAL_ERROR`.
    pub fn internal(message: impl Into<String>) -> Self {
        Self::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            code::INTERNAL_ERROR,
            message,
        )
    }

    /// `503 SERVICE_UNAVAILABLE`.
    pub fn unavailable(message: impl Into<String>) -> Self {
        Self::new(
            StatusCode::SERVICE_UNAVAILABLE,
            code::SERVICE_UNAVAILABLE,
            message,
        )
    }

    /// Appends a field-scoped detail.
    pub fn with_detail(mut self, field: impl Into<String>, message: impl Into<String>) -> Self {
        self.body.details.push(Detail {
            field: Some(field.into()),
            message: message.into(),
        });
        self
    }

    /// Appends a detail with no field name.
    pub fn with_message(mut self, message: impl Into<String>) -> Self {
        self.body.details.push(Detail {
            field: None,
            message: message.into(),
        });
        self
    }

    /// Maps a status to the default code for that class.
    ///
    /// Used by call sites that already hold a [`StatusCode`] and only want the
    /// envelope.
    pub fn from_status(status: StatusCode, message: impl Into<String>) -> Self {
        let code = match status {
            StatusCode::BAD_REQUEST => code::BAD_REQUEST,
            StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => code::UNAUTHORIZED,
            StatusCode::NOT_FOUND => code::NOT_FOUND,
            StatusCode::CONFLICT => code::CONFLICT,
            StatusCode::TOO_MANY_REQUESTS => code::RATE_LIMITED,
            StatusCode::BAD_GATEWAY | StatusCode::GATEWAY_TIMEOUT => code::UPSTREAM_ERROR,
            StatusCode::SERVICE_UNAVAILABLE => code::SERVICE_UNAVAILABLE,
            _ => code::INTERNAL_ERROR,
        };
        Self::new(status, code, message)
    }
}

/// Wire wrapper: `{"error": {...}}`.
#[derive(Serialize)]
struct Envelope<'a> {
    error: &'a Body,
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.status, Json(Envelope { error: &self.body })).into_response()
    }
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.body.code, self.body.message)
    }
}

impl std::error::Error for ApiError {}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::to_bytes;
    use axum::response::IntoResponse;

    async fn body_of(err: ApiError) -> (u16, serde_json::Value) {
        let res = err.into_response();
        let status = res.status().as_u16();
        let bytes = to_bytes(res.into_body(), 64 * 1024).await.unwrap();
        (status, serde_json::from_slice(&bytes).unwrap())
    }

    #[tokio::test]
    async fn serializes_the_documented_envelope() {
        let (status, body) = body_of(
            ApiError::validation("Invalid input").with_detail("email", "Invalid email format"),
        )
        .await;
        assert_eq!(status, 400);
        assert_eq!(body["error"]["code"], "VALIDATION_ERROR");
        assert_eq!(body["error"]["message"], "Invalid input");
        assert_eq!(body["error"]["details"][0]["field"], "email");
        assert_eq!(
            body["error"]["details"][0]["message"],
            "Invalid email format"
        );
        // No stray top-level keys.
        assert_eq!(body.as_object().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn details_are_omitted_when_empty() {
        let (_, body) = body_of(ApiError::not_found("no such case")).await;
        assert!(body["error"].get("details").is_none());
        assert_eq!(body["error"]["code"], "NOT_FOUND");
    }

    #[tokio::test]
    async fn status_maps_to_the_expected_code() {
        let cases = [
            (StatusCode::BAD_REQUEST, "BAD_REQUEST"),
            (StatusCode::UNAUTHORIZED, "UNAUTHORIZED"),
            (StatusCode::NOT_FOUND, "NOT_FOUND"),
            (StatusCode::CONFLICT, "CONFLICT"),
            (StatusCode::TOO_MANY_REQUESTS, "RATE_LIMITED"),
            (StatusCode::BAD_GATEWAY, "UPSTREAM_ERROR"),
            (StatusCode::SERVICE_UNAVAILABLE, "SERVICE_UNAVAILABLE"),
            (StatusCode::INTERNAL_SERVER_ERROR, "INTERNAL_ERROR"),
        ];
        for (status, expected) in cases {
            let err = ApiError::from_status(status, "x");
            assert_eq!(err.status, status);
            assert_eq!(err.body.code, expected, "status {status}");
        }
    }

    #[test]
    fn constructors_pick_sensible_statuses() {
        assert_eq!(ApiError::bad_request("x").status, StatusCode::BAD_REQUEST);
        assert_eq!(ApiError::conflict("x").status, StatusCode::CONFLICT);
        assert_eq!(ApiError::upstream("x").status, StatusCode::BAD_GATEWAY);
        assert_eq!(
            ApiError::internal("x").status,
            StatusCode::INTERNAL_SERVER_ERROR
        );
        assert_eq!(ApiError::rate_limited("x").body.code, "RATE_LIMITED");
    }
}
