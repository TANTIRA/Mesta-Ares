//! One error envelope for the read-model API.
//!
//! Every handler answers failures as `{"error": "<message>"}` so the console
//! (and the documented contract) sees a single shape regardless of route.

use axum::{http::StatusCode, Json};
use serde_json::json;

/// Error type returned by read-model handlers.
pub type ApiError = (StatusCode, Json<serde_json::Value>);

/// Builds an [`ApiError`] with the standard body.
pub fn api_err(status: StatusCode, message: impl Into<String>) -> ApiError {
    (status, Json(json!({ "error": message.into() })))
}
