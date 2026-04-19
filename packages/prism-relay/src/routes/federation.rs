//! Federation peer routes.

use crate::relay_state::FullRelayState;
use axum::{extract::State, http::StatusCode, response::IntoResponse, Json};
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnnounceInput {
    pub relay_did: String,
    pub url: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ForwardInput {
    pub envelope: serde_json::Value,
    pub target_relay: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncInput {
    pub collection_id: String,
    pub snapshot: Option<String>,
}

pub async fn announce(
    State(state): State<Arc<FullRelayState>>,
    Json(input): Json<AnnounceInput>,
) -> impl IntoResponse {
    let now = crate::util::now_rfc3339();
    state
        .federation()
        .announce(&input.relay_did, &input.url, &now);
    StatusCode::OK
}

pub async fn list_peers(State(state): State<Arc<FullRelayState>>) -> impl IntoResponse {
    Json(json!(state.federation().get_peers()))
}

pub async fn forward(
    State(state): State<Arc<FullRelayState>>,
    Json(input): Json<ForwardInput>,
) -> impl IntoResponse {
    let result = state
        .federation()
        .forward_envelope(&input.envelope.to_string(), &input.target_relay);
    Json(json!(result))
}

pub async fn sync_receive(
    State(state): State<Arc<FullRelayState>>,
    Json(input): Json<SyncInput>,
) -> impl IntoResponse {
    let now = crate::util::now_rfc3339();
    if let Some(snapshot_b64) = &input.snapshot {
        if let Ok(data) = crate::util::b64_decode(snapshot_b64) {
            state
                .collections()
                .import_snapshot(&input.collection_id, data, &now);
        }
    }
    StatusCode::OK
}
