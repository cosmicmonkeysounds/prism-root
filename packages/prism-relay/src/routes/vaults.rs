//! Vault host routes.

use crate::relay_state::FullRelayState;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::Deserialize;
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Deserialize, Default)]
pub struct VaultListQuery {
    pub public: Option<bool>,
    pub search: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PublishVaultInput {
    pub manifest: serde_json::Value,
    pub owner_did: String,
    #[serde(default)]
    pub is_public: bool,
    #[serde(default)]
    pub collections: HashMap<String, String>, // id → base64 snapshot
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateCollectionsInput {
    pub owner_did: String,
    pub snapshots: HashMap<String, String>,
}

pub async fn list_vaults(
    State(state): State<Arc<FullRelayState>>,
    Query(query): Query<VaultListQuery>,
) -> impl IntoResponse {
    if let Some(ref q) = query.search {
        return Json(json!(state.vaults().search(q)));
    }
    Json(json!(state.vaults().list(query.public.unwrap_or(false))))
}

pub async fn publish_vault(
    State(state): State<Arc<FullRelayState>>,
    Json(input): Json<PublishVaultInput>,
) -> impl IntoResponse {
    use base64::Engine;
    let now = crate::util::now_rfc3339();
    let collections: HashMap<String, Vec<u8>> = input
        .collections
        .into_iter()
        .filter_map(|(k, v)| {
            base64::engine::general_purpose::STANDARD
                .decode(&v)
                .ok()
                .map(|d| (k, d))
        })
        .collect();
    let vault = state.vaults().publish(
        input.manifest,
        &input.owner_did,
        input.is_public,
        collections,
        &now,
    );
    (StatusCode::CREATED, Json(json!(vault)))
}

pub async fn get_vault(
    State(state): State<Arc<FullRelayState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match state.vaults().get(&id) {
        Some(v) => Ok(Json(json!(v))),
        None => Err(StatusCode::NOT_FOUND),
    }
}

pub async fn get_vault_collections(
    State(state): State<Arc<FullRelayState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match state.vaults().get_all_snapshots(&id) {
        Some(snaps) => {
            let listing: Vec<_> = snaps
                .iter()
                .map(|(k, v)| json!({"id": k, "size": v.len()}))
                .collect();
            Ok(Json(json!(listing)))
        }
        None => Err(StatusCode::NOT_FOUND),
    }
}

pub async fn get_vault_collection(
    State(state): State<Arc<FullRelayState>>,
    Path((vault_id, coll_id)): Path<(String, String)>,
) -> impl IntoResponse {
    match state.vaults().get_snapshot(&vault_id, &coll_id) {
        Some(data) => {
            use base64::Engine;
            Ok(Json(
                json!({"snapshot": base64::engine::general_purpose::STANDARD.encode(data)}),
            ))
        }
        None => Err(StatusCode::NOT_FOUND),
    }
}

pub async fn download_vault(
    State(state): State<Arc<FullRelayState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let vault = state.vaults().get(&id).ok_or(StatusCode::NOT_FOUND)?;
    let snaps = state.vaults().get_all_snapshots(&id).unwrap_or_default();
    use base64::Engine;
    let encoded: HashMap<String, String> = snaps
        .into_iter()
        .map(|(k, v)| (k, base64::engine::general_purpose::STANDARD.encode(v)))
        .collect();
    Ok::<_, StatusCode>(Json(
        json!({"manifest": vault.manifest, "collections": encoded}),
    ))
}

pub async fn update_vault_collections(
    State(state): State<Arc<FullRelayState>>,
    Path(id): Path<String>,
    Json(input): Json<UpdateCollectionsInput>,
) -> impl IntoResponse {
    use base64::Engine;
    let now = crate::util::now_rfc3339();
    let updates: HashMap<String, Vec<u8>> = input
        .snapshots
        .into_iter()
        .filter_map(|(k, v)| {
            base64::engine::general_purpose::STANDARD
                .decode(&v)
                .ok()
                .map(|d| (k, d))
        })
        .collect();
    if state
        .vaults()
        .update_collections(&id, &input.owner_did, updates, &now)
    {
        StatusCode::OK
    } else {
        StatusCode::FORBIDDEN
    }
}

pub async fn delete_vault(
    State(state): State<Arc<FullRelayState>>,
    Path(id): Path<String>,
    Json(body): Json<serde_json::Value>,
) -> impl IntoResponse {
    let owner = body.get("ownerDid").and_then(|v| v.as_str()).unwrap_or("");
    if state.vaults().remove(&id, owner) {
        StatusCode::OK
    } else {
        StatusCode::FORBIDDEN
    }
}
