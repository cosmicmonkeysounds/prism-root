//! Axum extractors. `RequireAuth` pulls the bearer token from the
//! `Authorization` header, verifies it, and yields the authenticated
//! username so handlers can ACL-check without re-parsing headers.

use std::sync::Arc;

use axum::{
    async_trait,
    extract::FromRequestParts,
    http::{header, request::Parts},
};

use crate::auth::decode_session_token;
use crate::error::ApiError;
use crate::LoomRelayState;

/// The authenticated caller. Username comes from the session token's
/// `subject` field.
pub struct RequireAuth {
    pub username: String,
}

#[async_trait]
impl FromRequestParts<Arc<LoomRelayState>> for RequireAuth {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &Arc<LoomRelayState>,
    ) -> Result<Self, Self::Rejection> {
        let header_value = parts
            .headers
            .get(header::AUTHORIZATION)
            .ok_or(ApiError::Unauthorized("missing Authorization header"))?
            .to_str()
            .map_err(|_| ApiError::Unauthorized("non-ASCII Authorization header"))?;
        let bearer = header_value
            .strip_prefix("Bearer ")
            .ok_or(ApiError::Unauthorized("expected `Bearer <token>`"))?
            .trim();

        let token = decode_session_token(&state.tokens(), bearer)?;
        Ok(RequireAuth {
            username: token.subject,
        })
    }
}
