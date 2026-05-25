//! Session-token machinery built on top of `CapabilityTokenManager`.
//!
//! A `SessionToken` is the base64-url JSON encoding of a fully-signed
//! `CapabilityToken` (scope `"session"`). The client carries it in
//! `Authorization: Bearer <token>` on every authenticated request.
//! Verification re-checks the SHA-256 payload signature via the
//! relay's `CapabilityTokenManager::verify` so the server cannot be
//! tricked by a forged token, and consults the revocation set so
//! logout can invalidate sessions.

use base64::Engine;
use chrono::{Duration, Utc};
use prism_core::network::relay::modules::capability_tokens::{
    CapabilityToken, CapabilityTokenManager,
};

use crate::error::ApiError;

/// 24h. Renew on subsequent requests if we want sliding sessions —
/// today the client must log in again after expiry.
const SESSION_TTL_HOURS: i64 = 24;

/// Scope tag used on every session-bearing capability token.
pub const SESSION_SCOPE: &str = "session";

/// Mint a new session token for `username` and return its base64-url
/// encoded form, ready to hand to the client.
pub fn mint_session_token(tokens: &CapabilityTokenManager, username: &str) -> String {
    let now = Utc::now();
    let expires_at = now + Duration::hours(SESSION_TTL_HOURS);
    let token = tokens.issue(
        username,
        vec!["read".into(), "write".into()],
        SESSION_SCOPE,
        &now.to_rfc3339(),
        Some(expires_at.to_rfc3339()),
    );
    encode_token(&token)
}

/// Decode a bearer string into a `CapabilityToken` and verify it
/// against the relay's signing key + revocation set. Also enforces
/// scope + expiry.
pub fn decode_session_token(
    tokens: &CapabilityTokenManager,
    bearer: &str,
) -> Result<CapabilityToken, ApiError> {
    let token = decode_token(bearer).map_err(|_| ApiError::Unauthorized("malformed token"))?;
    tokens
        .verify(&token)
        .map_err(|_| ApiError::Unauthorized("invalid token"))?;
    if token.scope != SESSION_SCOPE {
        return Err(ApiError::Unauthorized("wrong token scope"));
    }
    if let Some(exp) = &token.expires_at {
        let parsed = chrono::DateTime::parse_from_rfc3339(exp)
            .map_err(|_| ApiError::Unauthorized("malformed expiry"))?;
        if parsed < Utc::now() {
            return Err(ApiError::Unauthorized("session expired"));
        }
    }
    Ok(token)
}

fn encode_token(token: &CapabilityToken) -> String {
    let json = serde_json::to_vec(token).expect("CapabilityToken always serializes");
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(json)
}

fn decode_token(bearer: &str) -> anyhow::Result<CapabilityToken> {
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(bearer)?;
    let token = serde_json::from_slice(&bytes)?;
    Ok(token)
}
