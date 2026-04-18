//! Backup/restore routes.

use crate::persistence::RelayState;
use crate::relay_state::FullRelayState;
use axum::{extract::State, response::IntoResponse, Json};
use serde_json::json;
use std::sync::Arc;

pub async fn export_backup(State(state): State<Arc<FullRelayState>>) -> impl IntoResponse {
    let backup = RelayState {
        portals: state.portal_registry().list(),
        webhooks: state.webhooks().list(),
        templates: state.templates().list(),
        certificates: state.acme().list_certificates(),
        federation_peers: state.federation().get_peers(),
        flagged_content: state.trust().flagged_content(),
        peer_reputations: state.trust().all_peers(),
        escrow_deposits: Vec::new(),
        password_users: Vec::new(),
        revoked_tokens: state.tokens().revoked_ids(),
        collections: Vec::new(),
        vaults: state.vaults().list(false),
    };
    Json(json!(backup))
}

pub async fn import_backup(
    State(state): State<Arc<FullRelayState>>,
    Json(backup): Json<RelayState>,
) -> impl IntoResponse {
    state.portal_registry().restore(backup.portals.clone());
    state.webhooks().restore(backup.webhooks.clone());
    state.templates().restore(backup.templates.clone());
    state.acme().restore_certs(backup.certificates.clone());
    state.federation().restore(backup.federation_peers.clone());
    state.trust().restore(
        backup.peer_reputations.clone(),
        backup.flagged_content.clone(),
    );
    state
        .tokens()
        .restore_revoked(backup.revoked_tokens.clone());

    Json(json!({
        "restored": {
            "portals": backup.portals.len(),
            "webhooks": backup.webhooks.len(),
            "templates": backup.templates.len(),
            "certificates": backup.certificates.len(),
            "peers": backup.federation_peers.len(),
            "flaggedContent": backup.flagged_content.len(),
        }
    }))
}
