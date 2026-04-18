//! Status, health, and module listing routes.

use std::sync::Arc;

use axum::{extract::State, response::IntoResponse, Json};
use serde_json::json;

use crate::relay_state::FullRelayState;

pub async fn api_status(State(state): State<Arc<FullRelayState>>) -> impl IntoResponse {
    Json(json!({
        "running": true,
        "did": state.relay_did,
        "modules": state.relay.modules(),
        "peers": state.federation().get_peers().len(),
    }))
}

pub async fn api_modules(State(state): State<Arc<FullRelayState>>) -> impl IntoResponse {
    let modules: Vec<_> = state
        .relay
        .modules()
        .iter()
        .map(|m| json!({"name": m}))
        .collect();
    Json(json!(modules))
}

pub async fn api_health(State(state): State<Arc<FullRelayState>>) -> impl IntoResponse {
    let uptime = state.metrics.uptime_seconds();
    Json(json!({
        "running": true,
        "did": state.relay_did,
        "uptime": uptime,
        "peerCount": state.federation().get_peers().len(),
        "federationPeers": state.federation().get_peers().len(),
    }))
}
