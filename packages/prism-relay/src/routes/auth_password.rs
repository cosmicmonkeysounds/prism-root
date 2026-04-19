//! Password authentication routes.

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
) -> impl IntoResponse {
    let now = crate::util::now_rfc3339();
    match state.password_auth().register(
        &input.username,
        &input.password,
        input.did,
        input.metadata,
        &now,
    ) {
        Ok(record) => Ok((StatusCode::CREATED, Json(record.redacted()))),
        Err(_) => Err(StatusCode::CONFLICT),
    }
}

pub async fn login(
    State(state): State<Arc<FullRelayState>>,
    Json(input): Json<LoginInput>,
) -> impl IntoResponse {
    match state
        .password_auth()
        .login(&input.username, &input.password)
    {
        Ok(record) => Ok(Json(json!({"ok": true, "did": record.did}))),
        Err(_) => Err(StatusCode::UNAUTHORIZED),
    }
}

pub async fn change_password(
    State(state): State<Arc<FullRelayState>>,
    Json(input): Json<ChangePasswordInput>,
) -> impl IntoResponse {
    match state.password_auth().change_password(
        &input.username,
        &input.old_password,
        &input.new_password,
    ) {
        Ok(()) => StatusCode::OK,
        Err(_) => StatusCode::UNAUTHORIZED,
    }
}

pub async fn get_user(
    State(state): State<Arc<FullRelayState>>,
    Path(username): Path<String>,
) -> impl IntoResponse {
    match state.password_auth().get(&username) {
        Some(record) => Ok(Json(record.redacted())),
        None => Err(StatusCode::NOT_FOUND),
    }
}

pub async fn delete_user(
    State(state): State<Arc<FullRelayState>>,
    Path(username): Path<String>,
    Json(input): Json<DeleteInput>,
) -> impl IntoResponse {
    match state.password_auth().remove(&username, &input.password) {
        Ok(()) => StatusCode::OK,
        Err(_) => StatusCode::UNAUTHORIZED,
    }
}
