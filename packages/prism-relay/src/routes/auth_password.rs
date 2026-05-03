//! Password authentication routes.

use crate::relay_state::FullRelayState;
use crate::result::{RelayError, RelayResult};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;

#[derive(Deserialize)]
pub struct RegisterInput {
    pub username: String,
    pub password: String,
    pub did: Option<String>,
    pub metadata: Option<serde_json::Value>,
}

#[derive(Deserialize)]
pub struct LoginInput {
    pub username: String,
    pub password: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangePasswordInput {
    pub username: String,
    pub old_password: String,
    pub new_password: String,
}

#[derive(Deserialize)]
pub struct DeleteInput {
    pub password: String,
}

pub async fn register(
    State(state): State<Arc<FullRelayState>>,
    Json(input): Json<RegisterInput>,
) -> RelayResult<(StatusCode, Json<serde_json::Value>)> {
    let now = crate::util::now_rfc3339();
    let record = state
        .password_auth()
        .register(
            &input.username,
            &input.password,
            input.did,
            input.metadata,
            &now,
        )
        .map_err(|_| RelayError::conflict())?;
    Ok((StatusCode::CREATED, Json(record.redacted())))
}

pub async fn login(
    State(state): State<Arc<FullRelayState>>,
    Json(input): Json<LoginInput>,
) -> RelayResult<Json<serde_json::Value>> {
    let record = state
        .password_auth()
        .login(&input.username, &input.password)
        .map_err(|_| RelayError::unauthorized())?;
    Ok(Json(json!({"ok": true, "did": record.did})))
}

pub async fn change_password(
    State(state): State<Arc<FullRelayState>>,
    Json(input): Json<ChangePasswordInput>,
) -> RelayResult<StatusCode> {
    state
        .password_auth()
        .change_password(&input.username, &input.old_password, &input.new_password)
        .map_err(|_| RelayError::unauthorized())?;
    Ok(StatusCode::OK)
}

pub async fn get_user(
    State(state): State<Arc<FullRelayState>>,
    Path(username): Path<String>,
) -> RelayResult<Json<serde_json::Value>> {
    let record = state
        .password_auth()
        .get(&username)
        .ok_or_else(RelayError::not_found)?;
    Ok(Json(record.redacted()))
}

pub async fn delete_user(
    State(state): State<Arc<FullRelayState>>,
    Path(username): Path<String>,
    Json(input): Json<DeleteInput>,
) -> RelayResult<StatusCode> {
    state
        .password_auth()
        .remove(&username, &input.password)
        .map_err(|_| RelayError::unauthorized())?;
    Ok(StatusCode::OK)
}
