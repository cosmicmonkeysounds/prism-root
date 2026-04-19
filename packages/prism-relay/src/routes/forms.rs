//! Form submission routes for L3 portals.

use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde_json::json;

use crate::relay_state::FullRelayState;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FormSubmission {
    pub id: String,
    pub portal_id: String,
    pub data: serde_json::Value,
    pub submitted_at: String,
}

pub async fn submit_form(
    State(state): State<Arc<FullRelayState>>,
    Path(portal_id): Path<String>,
    Json(data): Json<serde_json::Value>,
) -> impl IntoResponse {
    let portal = state.portal_registry().get(&portal_id);
    if portal.is_none() {
        return Err(StatusCode::NOT_FOUND);
    }

    let now = crate::util::now_rfc3339();
    let submission = FormSubmission {
        id: format!("sub-{}", chrono::Utc::now().timestamp_millis()),
        portal_id: portal_id.clone(),
        data,
        submitted_at: now,
    };

    Ok((
        StatusCode::CREATED,
        Json(json!({
            "ok": true,
            "submissionId": submission.id,
            "portalId": submission.portal_id,
        })),
    ))
}

pub async fn list_submissions(
    State(state): State<Arc<FullRelayState>>,
    Path(portal_id): Path<String>,
) -> impl IntoResponse {
    if state.portal_registry().get(&portal_id).is_none() {
        return Err(StatusCode::NOT_FOUND);
    }
    Ok(Json(json!([])))
}
