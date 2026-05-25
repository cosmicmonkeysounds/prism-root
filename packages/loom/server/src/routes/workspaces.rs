//! `/api/workspaces/*` — multi-workspace CRUD + raw Loro snapshot
//! transport. Each workspace pairs metadata (here) with a CRDT
//! snapshot blob hosted by `collection_host` under the same id.
//!
//! ACL in v1 is owner-only: a workspace's owner is the only caller
//! who can read, write, or delete it. Sharing arrives in Phase 2.5
//! via capability tokens (`/api/tokens/issue`).

use std::sync::Arc;

use axum::{
    body::Bytes,
    extract::{Path, State},
    Json,
};
use chrono::Utc;
use serde::Serialize;

use crate::error::{ApiError, ApiResult};
use crate::extractors::RequireAuth;
use crate::workspaces::WorkspaceMeta;
use crate::LoomRelayState;

#[derive(serde::Deserialize)]
pub struct CreateRequest {
    pub name: String,
}

#[derive(Serialize)]
pub struct WorkspaceList {
    pub workspaces: Vec<WorkspaceMeta>,
}

#[derive(Serialize)]
pub struct OkResponse {
    pub ok: bool,
}

pub async fn list(
    State(state): State<Arc<LoomRelayState>>,
    auth: RequireAuth,
) -> Json<WorkspaceList> {
    Json(WorkspaceList {
        workspaces: state.workspaces.list_for_owner(&auth.username),
    })
}

pub async fn create(
    State(state): State<Arc<LoomRelayState>>,
    auth: RequireAuth,
    Json(body): Json<CreateRequest>,
) -> ApiResult<Json<WorkspaceMeta>> {
    let name = body.name.trim();
    if name.is_empty() {
        return Err(ApiError::BadRequest("name must not be empty".into()));
    }
    let now = Utc::now().to_rfc3339();
    let meta = state.workspaces.create(name, &auth.username, &now);
    // Reserve a slot in collection-host so snapshot POSTs land cleanly.
    state.collections().create(&meta.id, &now);
    Ok(Json(meta))
}

pub async fn get(
    State(state): State<Arc<LoomRelayState>>,
    auth: RequireAuth,
    Path(id): Path<String>,
) -> ApiResult<Json<WorkspaceMeta>> {
    let meta = state.workspaces.get(&id).ok_or(ApiError::NotFound)?;
    if meta.owner != auth.username {
        return Err(ApiError::Forbidden);
    }
    Ok(Json(meta))
}

pub async fn delete(
    State(state): State<Arc<LoomRelayState>>,
    auth: RequireAuth,
    Path(id): Path<String>,
) -> ApiResult<Json<OkResponse>> {
    let meta = state.workspaces.get(&id).ok_or(ApiError::NotFound)?;
    if meta.owner != auth.username {
        return Err(ApiError::Forbidden);
    }
    state.workspaces.remove(&id);
    state.collections().remove(&id);
    Ok(Json(OkResponse { ok: true }))
}

pub async fn get_snapshot(
    State(state): State<Arc<LoomRelayState>>,
    auth: RequireAuth,
    Path(id): Path<String>,
) -> ApiResult<Bytes> {
    let meta = state.workspaces.get(&id).ok_or(ApiError::NotFound)?;
    if meta.owner != auth.username {
        return Err(ApiError::Forbidden);
    }
    let bytes = state
        .collections()
        .export_snapshot(&id)
        .unwrap_or_default();
    Ok(Bytes::from(bytes))
}

pub async fn put_snapshot(
    State(state): State<Arc<LoomRelayState>>,
    auth: RequireAuth,
    Path(id): Path<String>,
    body: Bytes,
) -> ApiResult<Json<OkResponse>> {
    let meta = state.workspaces.get(&id).ok_or(ApiError::NotFound)?;
    if meta.owner != auth.username {
        return Err(ApiError::Forbidden);
    }
    let now = Utc::now().to_rfc3339();
    state
        .collections()
        .import_snapshot(&id, body.to_vec(), &now);
    Ok(Json(OkResponse { ok: true }))
}
