//! AutoREST API gateway — collection object CRUD.

use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::relay_state::FullRelayState;

#[derive(Deserialize, Default)]
pub struct ListQuery {
    #[serde(rename = "type")]
    pub object_type: Option<String>,
    pub status: Option<String>,
    pub tag: Option<String>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

pub async fn list_objects(
    State(state): State<Arc<FullRelayState>>,
    Path(collection_id): Path<String>,
    Query(_query): Query<ListQuery>,
) -> impl IntoResponse {
    match state.collections().get(&collection_id) {
        Some(_) => Ok(Json(json!([]))),
        None => Err(StatusCode::NOT_FOUND),
    }
}

pub async fn get_object(
    State(state): State<Arc<FullRelayState>>,
    Path((collection_id, _object_id)): Path<(String, String)>,
) -> impl IntoResponse {
    match state.collections().get(&collection_id) {
        Some(_) => Err::<Json<Value>, _>(StatusCode::NOT_FOUND),
        None => Err(StatusCode::NOT_FOUND),
    }
}

pub async fn create_object(
    State(state): State<Arc<FullRelayState>>,
    Path(collection_id): Path<String>,
    Json(body): Json<Value>,
) -> impl IntoResponse {
    if state.collections().get(&collection_id).is_none() {
        return Err(StatusCode::NOT_FOUND);
    }
    let now = chrono::Utc::now().to_rfc3339();
    state
        .webhooks()
        .emit("object.created", &body.to_string(), &now);
    Ok((StatusCode::CREATED, Json(body)))
}

pub async fn update_object(
    State(state): State<Arc<FullRelayState>>,
    Path((collection_id, _object_id)): Path<(String, String)>,
    Json(body): Json<Value>,
) -> impl IntoResponse {
    if state.collections().get(&collection_id).is_none() {
        return Err(StatusCode::NOT_FOUND);
    }
    let now = chrono::Utc::now().to_rfc3339();
    state
        .webhooks()
        .emit("object.updated", &body.to_string(), &now);
    Ok(Json(body))
}

pub async fn delete_object(
    State(state): State<Arc<FullRelayState>>,
    Path((collection_id, object_id)): Path<(String, String)>,
) -> impl IntoResponse {
    if state.collections().get(&collection_id).is_none() {
        return Err(StatusCode::NOT_FOUND);
    }
    let now = chrono::Utc::now().to_rfc3339();
    state.webhooks().emit(
        "object.deleted",
        &json!({"id": object_id}).to_string(),
        &now,
    );
    Ok(StatusCode::OK)
}
