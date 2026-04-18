//! Presence routes — multiplayer awareness.

use crate::relay_state::FullRelayState;
use axum::{extract::State, response::IntoResponse, Json};
use serde_json::json;
use std::sync::Arc;

pub async fn get_presence(State(_state): State<Arc<FullRelayState>>) -> impl IntoResponse {
    // Presence state is managed via WebSocket; HTTP endpoint returns current snapshot
    Json(json!([]))
}
