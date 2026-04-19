//! Blind ping (push notification) routes.

use crate::relay_state::FullRelayState;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use prism_core::network::relay::modules::blind_ping::{DeviceRegistration, PingPlatform};
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;

#[derive(Deserialize)]
pub struct RegisterInput {
    pub did: String,
    pub platform: PingPlatform,
    pub token: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SendPingInput {
    pub recipient_did: String,
    pub badge_count: Option<u32>,
}

pub async fn register_device(
    State(state): State<Arc<FullRelayState>>,
    Json(input): Json<RegisterInput>,
) -> impl IntoResponse {
    state.pinger().register(DeviceRegistration {
        did: input.did,
        platform: input.platform,
        token: input.token,
    });
    StatusCode::CREATED
}

pub async fn unregister_device(
    State(state): State<Arc<FullRelayState>>,
    Path(did): Path<String>,
) -> impl IntoResponse {
    state.pinger().unregister(&did);
    StatusCode::OK
}

pub async fn list_devices(State(state): State<Arc<FullRelayState>>) -> impl IntoResponse {
    Json(json!(state.pinger().devices()))
}

pub async fn send_ping(
    State(state): State<Arc<FullRelayState>>,
    Json(input): Json<SendPingInput>,
) -> impl IntoResponse {
    let now = crate::util::now_rfc3339();
    let ok = state
        .pinger()
        .ping(&input.recipient_did, input.badge_count, &now);
    Json(json!({"sent": ok}))
}

pub async fn wake(
    State(state): State<Arc<FullRelayState>>,
    Json(input): Json<SendPingInput>,
) -> impl IntoResponse {
    let now = crate::util::now_rfc3339();
    let ok = state
        .pinger()
        .wake(&input.recipient_did, input.badge_count, &now);
    Json(json!({"sent": ok}))
}
