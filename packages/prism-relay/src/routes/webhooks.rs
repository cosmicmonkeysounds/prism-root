//! Webhook CRUD + delivery tracking.

use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::Deserialize;
use serde_json::json;

use crate::relay_state::FullRelayState;

#[derive(Deserialize)]
pub struct CreateWebhookInput {
    pub url: String,
    pub events: Vec<String>,
    pub secret: Option<String>,
    #[serde(default = "default_true")]
    pub active: bool,
}

fn default_true() -> bool {
    true
}

pub async fn list_webhooks(State(state): State<Arc<FullRelayState>>) -> impl IntoResponse {
    Json(json!(state.webhooks().list()))
}

pub async fn create_webhook(
    State(state): State<Arc<FullRelayState>>,
    Json(input): Json<CreateWebhookInput>,
) -> impl IntoResponse {
    let wh = state
        .webhooks()
        .register(&input.url, input.events, input.secret);
    (StatusCode::CREATED, Json(json!(wh)))
}

pub async fn delete_webhook(
    State(state): State<Arc<FullRelayState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    if state.webhooks().unregister(&id) {
        StatusCode::OK
    } else {
        StatusCode::NOT_FOUND
    }
}

pub async fn get_deliveries(
    State(state): State<Arc<FullRelayState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    Json(json!(state.webhooks().deliveries(&id)))
}

pub async fn test_webhook(
    State(state): State<Arc<FullRelayState>>,
    Path(_id): Path<String>,
) -> impl IntoResponse {
    let now = chrono::Utc::now().to_rfc3339();
    let deliveries = state.webhooks().emit("test", r#"{"test": true}"#, &now);
    Json(json!(deliveries))
}
