//! OAuth/OIDC auth routes and escrow key derivation.

use std::sync::Arc;

use axum::{extract::State, http::StatusCode, response::IntoResponse, Json};
use prism_core::network::relay::modules::oauth::{OAuthIdentity, OAuthProviderKind};
use serde::Deserialize;
use serde_json::json;

use crate::relay_state::FullRelayState;

pub async fn list_providers(State(state): State<Arc<FullRelayState>>) -> impl IntoResponse {
    let providers: Vec<&str> = state
        .oauth()
        .list_providers()
        .into_iter()
        .map(|p| p.as_str())
        .collect();
    Json(json!(providers))
}

pub async fn google_redirect(State(state): State<Arc<FullRelayState>>) -> impl IntoResponse {
    provider_redirect(&state, OAuthProviderKind::Google)
}

pub async fn github_redirect(State(state): State<Arc<FullRelayState>>) -> impl IntoResponse {
    provider_redirect(&state, OAuthProviderKind::GitHub)
}

fn provider_redirect(
    state: &FullRelayState,
    provider: OAuthProviderKind,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let oauth = state.oauth();
    let now = chrono::Utc::now().to_rfc3339();
    let session = oauth.create_session(provider, &now);
    let url = oauth
        .build_auth_url(provider, &session.state)
        .ok_or(StatusCode::NOT_FOUND)?;
    Ok(Json(json!({
        "url": url,
        "state": session.state,
    })))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OAuthCallbackInput {
    pub code: String,
    pub state: String,
    pub provider_user_id: String,
    pub did: String,
    pub email: Option<String>,
    pub display_name: Option<String>,
}

pub async fn google_callback(
    State(state): State<Arc<FullRelayState>>,
    Json(input): Json<OAuthCallbackInput>,
) -> impl IntoResponse {
    provider_callback(&state, OAuthProviderKind::Google, input)
}

pub async fn github_callback(
    State(state): State<Arc<FullRelayState>>,
    Json(input): Json<OAuthCallbackInput>,
) -> impl IntoResponse {
    provider_callback(&state, OAuthProviderKind::GitHub, input)
}

fn provider_callback(
    state: &FullRelayState,
    provider: OAuthProviderKind,
    input: OAuthCallbackInput,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let oauth = state.oauth();

    let session = oauth
        .validate_session(&input.state)
        .ok_or(StatusCode::BAD_REQUEST)?;

    if session.provider != provider {
        return Err(StatusCode::BAD_REQUEST);
    }

    let now = chrono::Utc::now().to_rfc3339();
    let identity = OAuthIdentity {
        provider,
        provider_user_id: input.provider_user_id.clone(),
        email: input.email,
        display_name: input.display_name,
        did: input.did.clone(),
        linked_at: now,
    };
    oauth.link_identity(identity);

    Ok(Json(json!({
        "ok": true,
        "provider": provider.as_str(),
        "did": input.did,
        "providerUserId": input.provider_user_id,
    })))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EscrowDeriveInput {
    pub did: String,
    pub provider: String,
    pub provider_user_id: String,
    pub encrypted_payload: String,
}

pub async fn escrow_derive(
    State(state): State<Arc<FullRelayState>>,
    Json(input): Json<EscrowDeriveInput>,
) -> impl IntoResponse {
    let provider = match input.provider.as_str() {
        "google" => OAuthProviderKind::Google,
        "github" => OAuthProviderKind::GitHub,
        _ => return Err(StatusCode::BAD_REQUEST),
    };

    let oauth = state.oauth();
    if oauth
        .get_identity_by_provider(provider, &input.provider_user_id)
        .is_none()
    {
        return Err(StatusCode::UNAUTHORIZED);
    }

    let now = chrono::Utc::now().to_rfc3339();
    let deposit = state
        .escrow()
        .deposit(&input.did, &input.encrypted_payload, None, &now);

    Ok(Json(json!({
        "ok": true,
        "depositId": deposit.id,
    })))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EscrowRecoverInput {
    pub did: String,
    pub provider: String,
    pub provider_user_id: String,
}

pub async fn escrow_recover(
    State(state): State<Arc<FullRelayState>>,
    Json(input): Json<EscrowRecoverInput>,
) -> impl IntoResponse {
    let provider = match input.provider.as_str() {
        "google" => OAuthProviderKind::Google,
        "github" => OAuthProviderKind::GitHub,
        _ => return Err(StatusCode::BAD_REQUEST),
    };

    let oauth = state.oauth();
    if oauth
        .get_identity_by_provider(provider, &input.provider_user_id)
        .is_none()
    {
        return Err(StatusCode::UNAUTHORIZED);
    }

    let deposits = state.escrow().list_deposits(&input.did);
    Ok(Json(json!({
        "ok": true,
        "deposits": deposits,
    })))
}
