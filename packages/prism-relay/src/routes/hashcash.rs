//! Hashcash proof-of-work routes.

use crate::relay_state::FullRelayState;
use axum::{extract::State, response::IntoResponse, Json};
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;

#[derive(Deserialize)]
pub struct ChallengeInput {
    pub resource: String,
}

#[derive(Deserialize)]
pub struct VerifyInput {
    pub challenge: prism_core::identity::trust::types::HashcashChallenge,
    pub counter: u64,
    pub hash: String,
}

pub async fn create_challenge(
    State(state): State<Arc<FullRelayState>>,
    Json(input): Json<ChallengeInput>,
) -> impl IntoResponse {
    let challenge = state.hashcash().create_challenge(&input.resource);
    Json(json!(challenge))
}

pub async fn verify_proof(
    State(_state): State<Arc<FullRelayState>>,
    Json(_input): Json<serde_json::Value>,
) -> impl IntoResponse {
    // Simplified — in production, deserialize the full proof and verify
    Json(json!({"valid": true}))
}
