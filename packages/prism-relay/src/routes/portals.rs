//! Portal CRUD and view routes.

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

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreatePortalInput {
    pub name: String,
    pub level: u8,
    pub collection_id: String,
    #[serde(default = "default_base_path")]
    pub base_path: String,
    #[serde(default = "crate::util::default_true")]
    pub is_public: bool,
    pub domain: Option<String>,
    pub access_scope: Option<String>,
}

fn default_base_path() -> String {
    "/".into()
}

pub async fn list_portals(State(state): State<Arc<FullRelayState>>) -> impl IntoResponse {
    let portals = state.portal_registry().list();
    let items: Vec<_> = portals
        .iter()
        .map(|p| {
            json!({
                "portalId": p.portal_id,
                "name": p.name,
                "level": p.level,
                "basePath": p.base_path,
                "isPublic": p.is_public,
                "domain": p.domain,
                "accessScope": p.access_scope,
                "createdAt": p.created_at,
            })
        })
        .collect();
    Json(json!(items))
}

pub async fn create_portal(
    State(state): State<Arc<FullRelayState>>,
    Json(input): Json<CreatePortalInput>,
) -> impl IntoResponse {
    let now = crate::util::now_rfc3339();
    let manifest = state.portal_registry().register(
        &input.name,
        input.level,
        &input.collection_id,
        &input.base_path,
        input.is_public,
        input.domain,
        input.access_scope,
        &now,
    );
    (StatusCode::CREATED, Json(json!(manifest)))
}

pub async fn get_portal(
    State(state): State<Arc<FullRelayState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match state.portal_registry().get(&id) {
        Some(p) => Ok(Json(json!(p))),
        None => Err(StatusCode::NOT_FOUND),
    }
}

pub async fn delete_portal(
    State(state): State<Arc<FullRelayState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    if state.portal_registry().unregister(&id) {
        StatusCode::OK
    } else {
        StatusCode::NOT_FOUND
    }
}

pub async fn export_portal(
    State(state): State<Arc<FullRelayState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let Some(portal) = state.portal_registry().get(&id) else {
        return Err(StatusCode::NOT_FOUND);
    };
    let snapshot = state.collections().export_snapshot(&portal.collection_id);
    let encoded = snapshot.map(crate::util::b64_encode).unwrap_or_default();
    Ok(Json(json!({
        "portal": portal,
        "snapshot": encoded,
    })))
}
