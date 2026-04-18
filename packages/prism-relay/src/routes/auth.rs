//! OAuth/OIDC auth routes and escrow key derivation.

use axum::{http::StatusCode, response::IntoResponse, Json};
use serde::Deserialize;
use serde_json::json;

pub async fn list_providers() -> impl IntoResponse {
    Json(json!(["google", "github"]))
}

pub async fn google_redirect() -> impl IntoResponse {
    StatusCode::NOT_IMPLEMENTED
}

pub async fn github_redirect() -> impl IntoResponse {
    StatusCode::NOT_IMPLEMENTED
}

#[derive(Deserialize)]
pub struct OAuthCallback {
    pub code: String,
}

pub async fn google_callback(Json(_input): Json<OAuthCallback>) -> impl IntoResponse {
    StatusCode::NOT_IMPLEMENTED
}

pub async fn github_callback(Json(_input): Json<OAuthCallback>) -> impl IntoResponse {
    StatusCode::NOT_IMPLEMENTED
}

pub async fn escrow_derive() -> impl IntoResponse {
    StatusCode::NOT_IMPLEMENTED
}

pub async fn escrow_recover() -> impl IntoResponse {
    StatusCode::NOT_IMPLEMENTED
}
