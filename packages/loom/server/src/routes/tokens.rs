//! `/api/tokens/*` — mint and verify scoped capability tokens for
//! workspace sharing. Distinct from the session tokens issued by
//! `/api/auth/*` (those have scope `"session"`); share-link tokens
//! carry the workspace id in their `scope` field so the websocket
//! handler can later admit a guest into one specific workspace.

use std::sync::Arc;

use axum::{extract::State, Json};
use base64::Engine;
use chrono::{Duration, Utc};
use prism_core::network::relay::modules::capability_tokens::CapabilityToken;
use serde::{Deserialize, Serialize};

use crate::error::{ApiError, ApiResult};
use crate::extractors::RequireAuth;
use crate::LoomRelayState;

#[derive(Deserialize)]
pub struct IssueRequest {
    #[serde(rename = "workspaceId")]
    pub workspace_id: String,
    pub permissions: Vec<String>,
    #[serde(default, rename = "ttlSeconds")]
    pub ttl_seconds: Option<i64>,
}

#[derive(Serialize)]
pub struct IssueResponse {
    pub token: String,
    #[serde(rename = "tokenId")]
    pub token_id: String,
    #[serde(rename = "expiresAt")]
    pub expires_at: Option<String>,
}

pub async fn issue(
    State(state): State<Arc<LoomRelayState>>,
    auth: RequireAuth,
    Json(body): Json<IssueRequest>,
) -> ApiResult<Json<IssueResponse>> {
    let meta = state
        .workspaces
        .get(&body.workspace_id)
        .ok_or(ApiError::NotFound)?;
    if meta.owner != auth.username {
        return Err(ApiError::Forbidden);
    }

    let now = Utc::now();
    let expires_at = body
        .ttl_seconds
        .map(|secs| (now + Duration::seconds(secs)).to_rfc3339());
    let token = state.tokens().issue(
        &auth.username,
        body.permissions,
        &body.workspace_id,
        &now.to_rfc3339(),
        expires_at.clone(),
    );
    let encoded = encode(&token);

    Ok(Json(IssueResponse {
        token: encoded,
        token_id: token.token_id,
        expires_at,
    }))
}

#[derive(Deserialize)]
pub struct VerifyRequest {
    pub token: String,
}

#[derive(Serialize)]
pub struct VerifyResponse {
    pub valid: bool,
    pub subject: Option<String>,
    pub scope: Option<String>,
    pub permissions: Option<Vec<String>>,
    #[serde(rename = "expiresAt")]
    pub expires_at: Option<String>,
}

pub async fn verify(
    State(state): State<Arc<LoomRelayState>>,
    Json(body): Json<VerifyRequest>,
) -> Json<VerifyResponse> {
    let Ok(token) = decode(&body.token) else {
        return Json(VerifyResponse {
            valid: false,
            subject: None,
            scope: None,
            permissions: None,
            expires_at: None,
        });
    };
    let valid = state.tokens().verify(&token).is_ok();
    if !valid {
        return Json(VerifyResponse {
            valid: false,
            subject: None,
            scope: None,
            permissions: None,
            expires_at: None,
        });
    }
    Json(VerifyResponse {
        valid: true,
        subject: Some(token.subject),
        scope: Some(token.scope),
        permissions: Some(token.permissions),
        expires_at: token.expires_at,
    })
}

fn encode(token: &CapabilityToken) -> String {
    let json = serde_json::to_vec(token).expect("CapabilityToken always serializes");
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(json)
}

fn decode(s: &str) -> anyhow::Result<CapabilityToken> {
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(s)?;
    Ok(serde_json::from_slice(&bytes)?)
}
