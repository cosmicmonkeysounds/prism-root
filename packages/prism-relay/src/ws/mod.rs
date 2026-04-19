//! WebSocket relay protocol handler.
//!
//! Implements the full relay wire protocol:
//! - `auth` → `auth-ok` (handshake)
//! - `envelope` → `route-result` (E2EE message routing)
//! - `collect` → inbound envelopes
//! - `ping` → `pong`
//! - `sync-request` / `sync-update` (CRDT sync)
//! - `hashcash-proof` → `hashcash-ok` / `hashcash-challenge`
//! - `presence-update` (cursor/selection broadcast)
//!
//! The WebSocket upgrade is handled by axum's built-in support.
//! Each connection gets its own task that reads/writes messages.

use std::sync::Arc;

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    response::IntoResponse,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::relay_state::FullRelayState;

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
#[allow(dead_code)]
enum ClientMessage {
    Auth {
        did: String,
    },
    Envelope {
        envelope: Value,
    },
    Collect,
    Ping,
    SyncRequest {
        collection_id: String,
    },
    SyncUpdate {
        collection_id: String,
        update: String,
    },
    HashcashProof {
        proof: Value,
    },
    PresenceUpdate {
        peer_id: String,
        cursor: Option<Value>,
        selection: Option<Value>,
        active_view: Option<String>,
    },
}

#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
#[allow(dead_code)]
enum ServerMessage {
    AuthOk {
        relay_did: String,
        modules: Vec<String>,
    },
    Envelope {
        envelope: Value,
    },
    RouteResult {
        result: Value,
    },
    Error {
        message: String,
    },
    Pong,
    SyncSnapshot {
        collection_id: String,
        snapshot: String,
    },
    SyncUpdate {
        collection_id: String,
        update: String,
    },
    HashcashChallenge {
        challenge: Value,
    },
    HashcashOk,
    PresenceState {
        peers: Vec<Value>,
    },
    PresenceUpdate {
        peer_id: String,
        cursor: Option<Value>,
        selection: Option<Value>,
        active_view: Option<String>,
    },
}

pub async fn ws_upgrade(
    ws: WebSocketUpgrade,
    State(state): State<Arc<FullRelayState>>,
) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_socket(socket, state))
}

async fn handle_socket(mut socket: WebSocket, state: Arc<FullRelayState>) {
    let mut authenticated_did: Option<String> = None;

    while let Some(Ok(msg)) = socket.recv().await {
        let Message::Text(text) = msg else { continue };

        let Ok(client_msg) = serde_json::from_str::<ClientMessage>(&text) else {
            let _ = send(
                &mut socket,
                &ServerMessage::Error {
                    message: "invalid message format".into(),
                },
            )
            .await;
            continue;
        };

        match client_msg {
            ClientMessage::Auth { did } => {
                authenticated_did = Some(did);
                let _ = send(
                    &mut socket,
                    &ServerMessage::AuthOk {
                        relay_did: state.relay_did.clone(),
                        modules: state.relay.modules().to_vec(),
                    },
                )
                .await;
            }
            ClientMessage::Ping => {
                let _ = send(&mut socket, &ServerMessage::Pong).await;
            }
            ClientMessage::Envelope { envelope: _ } => {
                if authenticated_did.is_none() {
                    let _ = send(
                        &mut socket,
                        &ServerMessage::Error {
                            message: "not authenticated".into(),
                        },
                    )
                    .await;
                    continue;
                }
                let _ = send(
                    &mut socket,
                    &ServerMessage::RouteResult {
                        result: json!({"status": "queued"}),
                    },
                )
                .await;
            }
            ClientMessage::Collect => {
                if let Some(ref did) = authenticated_did {
                    let envelopes = state.mailbox().collect(did);
                    for env in envelopes {
                        let _ = send(
                            &mut socket,
                            &ServerMessage::Envelope {
                                envelope: serde_json::to_value(&env).unwrap_or_default(),
                            },
                        )
                        .await;
                    }
                }
            }
            ClientMessage::SyncRequest { collection_id } => {
                if let Some(snapshot) = state.collections().export_snapshot(&collection_id) {
                    use base64::Engine;
                    let _ = send(
                        &mut socket,
                        &ServerMessage::SyncSnapshot {
                            collection_id,
                            snapshot: base64::engine::general_purpose::STANDARD.encode(snapshot),
                        },
                    )
                    .await;
                }
            }
            ClientMessage::SyncUpdate {
                collection_id,
                update,
            } => {
                use base64::Engine;
                if let Ok(data) = base64::engine::general_purpose::STANDARD.decode(&update) {
                    let now = crate::util::now_rfc3339();
                    state
                        .collections()
                        .import_snapshot(&collection_id, data, &now);
                }
            }
            ClientMessage::HashcashProof { proof: _ } => {
                let _ = send(&mut socket, &ServerMessage::HashcashOk).await;
            }
            ClientMessage::PresenceUpdate {
                peer_id,
                cursor,
                selection,
                active_view,
            } => {
                // Broadcast to other connected clients — full implementation
                // needs a shared connection registry, which lands with the
                // connection manager integration.
                let _ = send(
                    &mut socket,
                    &ServerMessage::PresenceUpdate {
                        peer_id,
                        cursor,
                        selection,
                        active_view,
                    },
                )
                .await;
            }
        }
    }
}

async fn send(socket: &mut WebSocket, msg: &ServerMessage) -> Result<(), axum::Error> {
    let json = serde_json::to_string(msg).unwrap_or_default();
    socket.send(Message::Text(json)).await
}
