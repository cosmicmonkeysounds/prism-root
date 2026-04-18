//! Directory feed route.

use crate::relay_state::FullRelayState;
use axum::{extract::State, response::IntoResponse, Json};
use serde_json::json;
use std::sync::Arc;

pub async fn directory(State(state): State<Arc<FullRelayState>>) -> impl IntoResponse {
    Json(json!({
        "did": state.relay_did,
        "name": state.config.directory.name,
        "description": state.config.directory.description,
        "version": env!("CARGO_PKG_VERSION"),
        "modules": state.relay.modules(),
        "uptime": state.metrics.uptime_seconds(),
        "federation": state.config.federation.enabled,
        "portals": state.portal_registry().list_public(),
        "vaults": state.vaults().list(true),
    }))
}
