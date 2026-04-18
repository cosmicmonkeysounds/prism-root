//! ACME certificate routes.

use crate::relay_state::FullRelayState;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use prism_core::network::relay::modules::acme::{AcmeChallenge, SslCertificate};
use serde_json::json;
use std::sync::Arc;

pub async fn acme_challenge_response(
    State(state): State<Arc<FullRelayState>>,
    Path(token): Path<String>,
) -> impl IntoResponse {
    match state.acme().get_challenge(&token) {
        Some(c) => Ok(c.key_authorization),
        None => Err(StatusCode::NOT_FOUND),
    }
}

pub async fn add_challenge(
    State(state): State<Arc<FullRelayState>>,
    Json(challenge): Json<AcmeChallenge>,
) -> impl IntoResponse {
    state.acme().add_challenge(challenge);
    StatusCode::CREATED
}

pub async fn remove_challenge(
    State(state): State<Arc<FullRelayState>>,
    Path(token): Path<String>,
) -> impl IntoResponse {
    if state.acme().remove_challenge(&token) {
        StatusCode::OK
    } else {
        StatusCode::NOT_FOUND
    }
}

pub async fn list_certificates(State(state): State<Arc<FullRelayState>>) -> impl IntoResponse {
    Json(json!(state.acme().list_certificates()))
}

pub async fn add_certificate(
    State(state): State<Arc<FullRelayState>>,
    Json(cert): Json<SslCertificate>,
) -> impl IntoResponse {
    state.acme().set_certificate(cert);
    StatusCode::CREATED
}

pub async fn get_certificate(
    State(state): State<Arc<FullRelayState>>,
    Path(domain): Path<String>,
) -> impl IntoResponse {
    match state.acme().get_certificate(&domain) {
        Some(c) => Ok(Json(json!(c))),
        None => Err(StatusCode::NOT_FOUND),
    }
}
