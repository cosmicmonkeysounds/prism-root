//! Collection CRUD routes.

use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::Deserialize;
use serde_json::json;

use crate::relay_state::FullRelayState;
use crate::result::{RelayError, RelayResult};

#[derive(Deserialize)]
pub struct CreateCollectionInput {
    pub id: String,
}

#[derive(Deserialize)]
pub struct ImportSnapshotInput {
    pub data: String, // base64
}

pub async fn list_collections(State(state): State<Arc<FullRelayState>>) -> impl IntoResponse {
    Json(json!(state.collections().list()))
}

pub async fn create_collection(
    State(state): State<Arc<FullRelayState>>,
    Json(input): Json<CreateCollectionInput>,
) -> impl IntoResponse {
    let now = crate::util::now_rfc3339();
    if state.collections().create(&input.id, &now) {
        StatusCode::CREATED
    } else {
        StatusCode::CONFLICT
    }
}

pub async fn get_snapshot(
    State(state): State<Arc<FullRelayState>>,
    Path(id): Path<String>,
) -> RelayResult<Json<serde_json::Value>> {
    let data = state
        .collections()
        .export_snapshot(&id)
        .ok_or_else(RelayError::not_found)?;
    Ok(Json(json!({"snapshot": crate::util::b64_encode(data)})))
}

pub async fn import_snapshot(
    State(state): State<Arc<FullRelayState>>,
    Path(id): Path<String>,
    Json(input): Json<ImportSnapshotInput>,
) -> RelayResult<StatusCode> {
    let data = crate::util::b64_decode(&input.data).map_err(|_| RelayError::bad_request())?;
    let now = crate::util::now_rfc3339();
    state.collections().import_snapshot(&id, data, &now);
    Ok(StatusCode::OK)
}

pub async fn delete_collection(
    State(state): State<Arc<FullRelayState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    if state.collections().remove(&id) {
        StatusCode::OK
    } else {
        StatusCode::NOT_FOUND
    }
}
