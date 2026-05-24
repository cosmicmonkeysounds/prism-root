//! `GET /api/health` — liveness probe.
//!
//! Returns the relay's DID, the installed module list (so clients can
//! confirm the server has the capabilities they expect), and the
//! process start time.

use std::sync::Arc;

use axum::{extract::State, Json};
use serde::Serialize;

use crate::LoomRelayState;

#[derive(Serialize)]
pub struct HealthResponse {
    pub status: &'static str,
    #[serde(rename = "relayDid")]
    pub relay_did: String,
    pub modules: Vec<String>,
    #[serde(rename = "startedAt")]
    pub started_at: String,
}

pub async fn health(State(state): State<Arc<LoomRelayState>>) -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        relay_did: state.relay_did.clone(),
        modules: state.relay.modules().to_vec(),
        started_at: state.started_at.clone(),
    })
}
