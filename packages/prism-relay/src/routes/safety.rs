//! Trust & safety — content flagging, toxic hash gossip.

use std::sync::Arc;

use axum::{extract::State, http::StatusCode, response::IntoResponse, Json};
use serde::Deserialize;
use serde_json::json;

use crate::relay_state::FullRelayState;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReportInput {
    pub content_hash: String,
    pub category: String,
    pub reporter: String,
}

#[derive(Deserialize)]
pub struct CheckHashesInput {
    pub hashes: Vec<String>,
}

pub async fn report(
    State(state): State<Arc<FullRelayState>>,
    Json(input): Json<ReportInput>,
) -> impl IntoResponse {
    let now = crate::util::now_rfc3339();
    state
        .trust()
        .flag_content(&input.content_hash, &input.category, &input.reporter, &now);
    StatusCode::CREATED
}

pub async fn list_hashes(State(state): State<Arc<FullRelayState>>) -> impl IntoResponse {
    Json(json!(state.trust().flagged_content()))
}

pub async fn import_hashes(
    State(state): State<Arc<FullRelayState>>,
    Json(items): Json<Vec<prism_core::network::relay::modules::peer_trust::FlaggedContent>>,
) -> impl IntoResponse {
    state.trust().import_flagged(items);
    StatusCode::OK
}

pub async fn check_hashes(
    State(state): State<Arc<FullRelayState>>,
    Json(input): Json<CheckHashesInput>,
) -> impl IntoResponse {
    Json(json!(state.trust().check_hashes(&input.hashes)))
}

pub async fn gossip_hashes(State(state): State<Arc<FullRelayState>>) -> impl IntoResponse {
    let hashes = state.trust().flagged_content();
    // In production, push to federation peers
    Json(json!({"pushed": hashes.len()}))
}
