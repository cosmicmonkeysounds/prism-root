//! WebRTC signaling routes.

use crate::relay_state::FullRelayState;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JoinInput {
    pub peer_id: String,
    pub display_name: Option<String>,
    pub metadata: Option<serde_json::Value>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LeaveInput {
    pub peer_id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SignalInput {
    #[serde(rename = "type")]
    pub signal_type: String,
    pub from: String,
    pub to: String,
    pub payload: serde_json::Value,
}

pub async fn list_rooms(State(state): State<Arc<FullRelayState>>) -> impl IntoResponse {
    let rooms: Vec<_> = state
        .signaling()
        .list_rooms()
        .into_iter()
        .map(|(id, count, created)| json!({"roomId": id, "peerCount": count, "createdAt": created}))
        .collect();
    Json(json!(rooms))
}

pub async fn get_peers(
    State(state): State<Arc<FullRelayState>>,
    Path(room_id): Path<String>,
) -> impl IntoResponse {
    Json(json!(state.signaling().get_peers(&room_id)))
}

pub async fn join_room(
    State(state): State<Arc<FullRelayState>>,
    Path(room_id): Path<String>,
    Json(input): Json<JoinInput>,
) -> impl IntoResponse {
    use prism_core::network::relay::modules::signaling::SignalingPeer;
    let now = crate::util::now_rfc3339();
    let existing = state.signaling().join(
        &room_id,
        SignalingPeer {
            peer_id: input.peer_id,
            display_name: input.display_name,
            joined_at: now.clone(),
            metadata: input.metadata,
        },
        Box::new(|_| {}),
        &now,
    );
    (StatusCode::CREATED, Json(json!(existing)))
}

pub async fn leave_room(
    State(state): State<Arc<FullRelayState>>,
    Path(room_id): Path<String>,
    Json(input): Json<LeaveInput>,
) -> impl IntoResponse {
    let now = crate::util::now_rfc3339();
    state.signaling().leave(&room_id, &input.peer_id, &now);
    StatusCode::OK
}

pub async fn relay_signal(
    State(state): State<Arc<FullRelayState>>,
    Path(room_id): Path<String>,
    Json(input): Json<SignalInput>,
) -> impl IntoResponse {
    use prism_core::network::relay::modules::signaling::{SignalMessage, SignalType};
    let now = crate::util::now_rfc3339();
    let signal_type = match input.signal_type.as_str() {
        "offer" => SignalType::Offer,
        "answer" => SignalType::Answer,
        "ice-candidate" => SignalType::IceCandidate,
        _ => SignalType::Leave,
    };
    let msg = SignalMessage {
        signal_type,
        from: input.from,
        to: input.to,
        room_id,
        payload: input.payload,
        timestamp: now,
    };
    let relayed = state.signaling().relay_signal(&msg);
    Json(json!({"relayed": relayed}))
}
