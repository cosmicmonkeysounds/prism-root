//! AutoREST API gateway — collection object CRUD.

use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::relay_state::FullRelayState;
use crate::result::{RelayError, RelayResult};

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
) -> RelayResult<Json<Value>> {
    state
        .collections()
        .get(&collection_id)
        .ok_or_else(RelayError::not_found)?;
    Ok(Json(json!([])))
}

pub async fn get_object(
    State(_state): State<Arc<FullRelayState>>,
    Path((_collection_id, _object_id)): Path<(String, String)>,
) -> RelayResult<Json<Value>> {
    Err(RelayError::not_found())
}

pub async fn create_object(
    State(state): State<Arc<FullRelayState>>,
    Path(collection_id): Path<String>,
    Json(body): Json<Value>,
) -> RelayResult<(StatusCode, Json<Value>)> {
    state
        .collections()
        .get(&collection_id)
        .ok_or_else(RelayError::not_found)?;
    let now = crate::util::now_rfc3339();
    state
        .webhooks()
        .emit("object.created", &body.to_string(), &now);
    Ok((StatusCode::CREATED, Json(body)))
}

pub async fn update_object(
    State(state): State<Arc<FullRelayState>>,
    Path((collection_id, _object_id)): Path<(String, String)>,
    Json(body): Json<Value>,
) -> RelayResult<Json<Value>> {
    state
        .collections()
        .get(&collection_id)
        .ok_or_else(RelayError::not_found)?;
    let now = crate::util::now_rfc3339();
    state
        .webhooks()
        .emit("object.updated", &body.to_string(), &now);
    Ok(Json(body))
}

pub async fn delete_object(
    State(state): State<Arc<FullRelayState>>,
    Path((collection_id, object_id)): Path<(String, String)>,
) -> RelayResult<StatusCode> {
    state
        .collections()
        .get(&collection_id)
        .ok_or_else(RelayError::not_found)?;
    let now = crate::util::now_rfc3339();
    state.webhooks().emit(
        "object.deleted",
        &json!({"id": object_id}).to_string(),
        &now,
    );
    Ok(StatusCode::OK)
}
