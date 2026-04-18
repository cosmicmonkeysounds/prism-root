//! Email transport routes.

use axum::{http::StatusCode, response::IntoResponse, Json};
use serde_json::json;

pub async fn email_status() -> impl IntoResponse {
    Json(json!({"configured": false}))
}

pub async fn send_email() -> impl IntoResponse {
    StatusCode::SERVICE_UNAVAILABLE
}
