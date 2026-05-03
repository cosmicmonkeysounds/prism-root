//! Form submission routes for L3 portals.

use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde_json::json;

use crate::relay_state::FullRelayState;
use crate::result::{RelayError, RelayResult};

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
) -> RelayResult<(StatusCode, Json<serde_json::Value>)> {
    state
        .portal_registry()
        .get(&portal_id)
        .ok_or_else(RelayError::not_found)?;

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
) -> RelayResult<Json<serde_json::Value>> {
    state
        .portal_registry()
        .get(&portal_id)
        .ok_or_else(RelayError::not_found)?;
    Ok(Json(json!([])))
}
