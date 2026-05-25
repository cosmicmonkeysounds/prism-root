//! `/api/auth/*` — username/password registration, login, and
//! password change. All three flows mint a fresh session token on
//! success; the client stores it and replays it as the bearer for
//! authenticated requests.

use std::sync::Arc;

use axum::{extract::State, Json};
use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::auth::mint_session_token;
use crate::error::{ApiError, ApiResult};
use crate::LoomRelayState;

#[derive(Deserialize)]
pub struct RegisterRequest {
    pub username: String,
    pub password: String,
}

#[derive(Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

#[derive(Deserialize)]
pub struct ChangeRequest {
    pub username: String,
    #[serde(rename = "oldPassword")]
    pub old_password: String,
    #[serde(rename = "newPassword")]
    pub new_password: String,
}

#[derive(Serialize)]
pub struct AuthResponse {
    pub username: String,
    #[serde(rename = "sessionToken")]
    pub session_token: String,
}

pub async fn register(
    State(state): State<Arc<LoomRelayState>>,
    Json(body): Json<RegisterRequest>,
) -> ApiResult<Json<AuthResponse>> {
    if body.username.trim().is_empty() || body.password.is_empty() {
        return Err(ApiError::BadRequest(
            "username and password are required".into(),
        ));
    }
    let now = Utc::now().to_rfc3339();
    state
        .password_auth()
        .register(&body.username, &body.password, None, None, &now)
        .map_err(ApiError::Conflict)?;
    let token = mint_session_token(&state.tokens(), &body.username);
    Ok(Json(AuthResponse {
        username: body.username,
        session_token: token,
    }))
}

pub async fn login(
    State(state): State<Arc<LoomRelayState>>,
    Json(body): Json<LoginRequest>,
) -> ApiResult<Json<AuthResponse>> {
    state
        .password_auth()
        .login(&body.username, &body.password)
        .map_err(|_| ApiError::Unauthorized("invalid credentials"))?;
    let token = mint_session_token(&state.tokens(), &body.username);
    Ok(Json(AuthResponse {
        username: body.username,
        session_token: token,
    }))
}

#[derive(Serialize)]
pub struct OkResponse {
    pub ok: bool,
}

pub async fn change(
    State(state): State<Arc<LoomRelayState>>,
    Json(body): Json<ChangeRequest>,
) -> ApiResult<Json<OkResponse>> {
    state
        .password_auth()
        .change_password(&body.username, &body.old_password, &body.new_password)
        .map_err(|_| ApiError::Unauthorized("invalid credentials"))?;
    Ok(Json(OkResponse { ok: true }))
}
