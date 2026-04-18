//! Log query routes (ring buffer).

use axum::{extract::Query, http::StatusCode, response::IntoResponse, Json};
use serde::Deserialize;
use serde_json::json;

#[derive(Deserialize, Default)]
pub struct LogQuery {
    pub level: Option<String>,
    pub limit: Option<usize>,
}

pub async fn get_logs(Query(_query): Query<LogQuery>) -> impl IntoResponse {
    // Ring buffer logging is a follow-on — return empty for now
    Json(json!([]))
}

pub async fn clear_logs() -> impl IntoResponse {
    StatusCode::OK
}
