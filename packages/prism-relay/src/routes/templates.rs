//! Portal template routes.

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
pub struct CreateTemplateInput {
    pub name: String,
    pub description: String,
    pub css: String,
    pub header_html: String,
    pub footer_html: String,
    pub object_card_html: String,
}

pub async fn list_templates(State(state): State<Arc<FullRelayState>>) -> impl IntoResponse {
    Json(json!(state.templates().list()))
}

pub async fn create_template(
    State(state): State<Arc<FullRelayState>>,
    Json(input): Json<CreateTemplateInput>,
) -> impl IntoResponse {
    let now = crate::util::now_rfc3339();
    let tpl = state.templates().register(
        &input.name,
        &input.description,
        &input.css,
        &input.header_html,
        &input.footer_html,
        &input.object_card_html,
        &now,
    );
    (StatusCode::CREATED, Json(json!(tpl)))
}

pub async fn get_template(
    State(state): State<Arc<FullRelayState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match state.templates().get(&id) {
        Some(t) => Ok(Json(json!(t))),
        None => Err(StatusCode::NOT_FOUND),
    }
}

pub async fn delete_template(
    State(state): State<Arc<FullRelayState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    if state.templates().remove(&id) {
        StatusCode::OK
    } else {
        StatusCode::NOT_FOUND
    }
}
