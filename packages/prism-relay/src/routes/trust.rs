//! Trust & peer management routes.

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
use crate::result::{RelayError, RelayResult};

#[derive(Deserialize)]
pub struct BanInput {
    pub reason: String,
}

pub async fn list_trust(State(state): State<Arc<FullRelayState>>) -> impl IntoResponse {
    Json(json!(state.trust().all_peers()))
}

pub async fn get_peer_trust(
    State(state): State<Arc<FullRelayState>>,
    Path(did): Path<String>,
) -> RelayResult<Json<serde_json::Value>> {
    let peer = state
        .trust()
        .get_peer(&did)
        .ok_or_else(RelayError::not_found)?;
    Ok(Json(json!(peer)))
}

pub async fn ban_peer(
    State(state): State<Arc<FullRelayState>>,
    Path(did): Path<String>,
    Json(input): Json<BanInput>,
) -> impl IntoResponse {
    let now = crate::util::now_rfc3339();
    state.trust().ban(&did, &input.reason, &now);
    StatusCode::OK
}

pub async fn unban_peer(
    State(state): State<Arc<FullRelayState>>,
    Path(did): Path<String>,
) -> impl IntoResponse {
    state.trust().unban(&did);
    StatusCode::OK
}
