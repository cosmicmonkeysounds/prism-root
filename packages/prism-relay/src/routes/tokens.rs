//! Capability token management.

use std::sync::Arc;

use axum::{extract::State, http::StatusCode, response::IntoResponse, Json};
use serde::Deserialize;
use serde_json::json;

use crate::relay_state::FullRelayState;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IssueTokenInput {
    pub subject: String,
    pub permissions: Vec<String>,
    pub scope: String,
    pub ttl_ms: Option<u64>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RevokeTokenInput {
    pub token_id: String,
}

pub async fn list_tokens(State(state): State<Arc<FullRelayState>>) -> impl IntoResponse {
    Json(json!(state.tokens().list()))
}

pub async fn issue_token(
    State(state): State<Arc<FullRelayState>>,
    Json(input): Json<IssueTokenInput>,
) -> impl IntoResponse {
    let now = crate::util::now_rfc3339();
    let expires = input.ttl_ms.map(|ttl| {
        let exp = chrono::Utc::now() + chrono::Duration::milliseconds(ttl as i64);
        exp.to_rfc3339()
    });
    let token = state.tokens().issue(
        &input.subject,
        input.permissions,
        &input.scope,
        &now,
        expires,
    );
    (StatusCode::CREATED, Json(json!(token)))
}

pub async fn verify_token(
    State(state): State<Arc<FullRelayState>>,
    Json(token): Json<prism_core::network::relay::modules::capability_tokens::CapabilityToken>,
) -> impl IntoResponse {
    match state.tokens().verify(&token) {
        Ok(()) => Json(json!({"valid": true})),
        Err(reason) => Json(json!({"valid": false, "reason": reason})),
    }
}

pub async fn revoke_token(
    State(state): State<Arc<FullRelayState>>,
    Json(input): Json<RevokeTokenInput>,
) -> impl IntoResponse {
    state.tokens().revoke(&input.token_id);
    StatusCode::OK
}
