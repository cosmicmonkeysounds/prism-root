//! Typed error wrapper for relay HTTP handlers.
//!
//! Replaces the per-handler `match … { Ok(v) => Ok(Json(json!(v))),
//! Err(_) => Err(StatusCode::CONFLICT) }` shape with a `RelayResult<T>`
//! whose `Err` arm carries optional structured error context.
//!
//! `RelayError` implements `IntoResponse` directly, and `From<StatusCode>`
//! to keep migration of existing handlers a one-line `.map_err(|_| …)?`
//! away.

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;

/// Standard handler return type. `T` is whatever success body the handler
/// produces (typically `Json<V>` or `(StatusCode, Json<V>)`).
pub type RelayResult<T> = Result<T, RelayError>;

/// Error body. Renders as `{ "error": "<msg>" }` with the chosen status
/// code; if `message` is `None`, the body is omitted (matches the legacy
/// "bare StatusCode" handler shape).
#[derive(Debug)]
pub struct RelayError {
    status: StatusCode,
    message: Option<String>,
}

impl RelayError {
    pub fn new(status: StatusCode) -> Self {
        Self {
            status,
            message: None,
        }
    }

    pub fn with_message(status: StatusCode, message: impl Into<String>) -> Self {
        Self {
            status,
            message: Some(message.into()),
        }
    }

    pub fn not_found() -> Self {
        Self::new(StatusCode::NOT_FOUND)
    }

    pub fn unauthorized() -> Self {
        Self::new(StatusCode::UNAUTHORIZED)
    }

    pub fn forbidden() -> Self {
        Self::new(StatusCode::FORBIDDEN)
    }

    pub fn bad_request() -> Self {
        Self::new(StatusCode::BAD_REQUEST)
    }

    pub fn bad_request_msg(msg: impl Into<String>) -> Self {
        Self::with_message(StatusCode::BAD_REQUEST, msg)
    }

    pub fn conflict() -> Self {
        Self::new(StatusCode::CONFLICT)
    }

    pub fn conflict_msg(msg: impl Into<String>) -> Self {
        Self::with_message(StatusCode::CONFLICT, msg)
    }

    pub fn internal(msg: impl Into<String>) -> Self {
        Self::with_message(StatusCode::INTERNAL_SERVER_ERROR, msg)
    }

    pub fn service_unavailable() -> Self {
        Self::new(StatusCode::SERVICE_UNAVAILABLE)
    }
}

impl IntoResponse for RelayError {
    fn into_response(self) -> Response {
        match self.message {
            Some(msg) => (self.status, Json(json!({ "error": msg }))).into_response(),
            None => self.status.into_response(),
        }
    }
}

impl From<StatusCode> for RelayError {
    fn from(status: StatusCode) -> Self {
        Self::new(status)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::to_bytes;

    #[tokio::test]
    async fn error_with_message_renders_json_error() {
        let resp = RelayError::conflict_msg("dup").into_response();
        assert_eq!(resp.status(), StatusCode::CONFLICT);
        let bytes = to_bytes(resp.into_body(), 4096).await.unwrap();
        assert_eq!(&bytes[..], br#"{"error":"dup"}"#);
    }

    #[tokio::test]
    async fn bare_status_error_has_empty_body() {
        let resp = RelayError::not_found().into_response();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
        let bytes = to_bytes(resp.into_body(), 4096).await.unwrap();
        assert!(bytes.is_empty());
    }

    #[tokio::test]
    async fn from_status_preserves_status_with_no_body() {
        let err: RelayError = StatusCode::BAD_REQUEST.into();
        let resp = err.into_response();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        let bytes = to_bytes(resp.into_body(), 4096).await.unwrap();
        assert!(bytes.is_empty());
    }
}
