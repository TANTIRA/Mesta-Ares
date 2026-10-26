//! One error envelope for the read-model API.
//!
//! Every handler answers failures with the shared
//! [`http_limits::error::ApiError`] envelope:
//!
//! ```json
//! { "error": { "code": "UPSTREAM_ERROR", "message": "opensearch: timeout" } }
//! ```
//!
//! so the console and the OpenAPI contract see one shape on every route.

use axum::http::StatusCode;

/// Error type returned by read-model handlers.
pub type ApiError = http_limits::error::ApiError;

/// Builds an [`ApiError`], deriving the `code` from `status`.
pub fn api_err(status: StatusCode, message: impl Into<String>) -> ApiError {
    ApiError::from_status(status, message)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn api_err_derives_the_code_from_status() {
        let e = api_err(StatusCode::BAD_GATEWAY, "nvd down");
        assert_eq!(e.status, StatusCode::BAD_GATEWAY);
        assert_eq!(e.body.code, "UPSTREAM_ERROR");
        assert_eq!(e.body.message, "nvd down");
        assert!(e.body.details.is_empty());
    }
}
