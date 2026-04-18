//! Escrow deposit/claim routes.

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
pub struct DepositInput {
    pub depositor_id: String,
    pub encrypted_payload: String,
    pub expires_at: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaimInput {
    pub deposit_id: String,
}

pub async fn deposit(
    State(state): State<Arc<FullRelayState>>,
    Json(input): Json<DepositInput>,
) -> impl IntoResponse {
    let now = chrono::Utc::now().to_rfc3339();
    let dep = state.escrow().deposit(
        &input.depositor_id,
        &input.encrypted_payload,
        input.expires_at,
        &now,
    );
    (StatusCode::CREATED, Json(json!(dep)))
}

pub async fn claim(
    State(state): State<Arc<FullRelayState>>,
    Json(input): Json<ClaimInput>,
) -> impl IntoResponse {
    match state.escrow().claim(&input.deposit_id) {
        Some(dep) => Ok(Json(json!(dep))),
        None => Err(StatusCode::NOT_FOUND),
    }
}

pub async fn list_deposits(
    State(state): State<Arc<FullRelayState>>,
    Path(depositor_id): Path<String>,
) -> impl IntoResponse {
    Json(json!(state.escrow().list_deposits(&depositor_id)))
}
