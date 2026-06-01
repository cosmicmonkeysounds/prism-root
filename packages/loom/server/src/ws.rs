//! `/ws` — Phase 3 + Phase 5 of the Loom multi-user backbone.
//!
//! One WebSocket per browser tab. The lifecycle is strict:
//!
//!   1. Client connects, server assigns a per-connection `peer_id`
//!      (uuid v4) but does not trust it for anything yet.
//!   2. Client sends `auth { token }`. Tokens are accepted in two
//!      shapes — a session token (scope `session`) minted by
//!      `/api/auth/{register,login}`, or a workspace capability token
//!      whose `scope` matches the workspace the client later
//!      subscribes to (the "share-link guest" path).
//!   3. After `auth-ok`, the client may `subscribe { workspace }` to
//!      one or more workspaces. The first subscribe-to-`W` triggers
//!      an ACL check; success emits a one-shot `snapshot` with the
//!      current `CollectionHost::export_snapshot` and joins the
//!      workspace's `tokio::sync::broadcast` channel.
//!   4. `update` envelopes carry a base64 CRDT delta. The server
//!      overwrites its canonical snapshot via
//!      `CollectionHost::import_snapshot` (the v1 store is opaque-blob,
//!      not a Loro merge — clients do the actual merge locally) and
//!      rebroadcasts to every other subscriber in the workspace.
//!   5. `presence` envelopes carry a full `PresenceState`. The server
//!      stores them in a per-workspace `RwLock<HashMap<peer_id, …>>`
//!      and rebroadcasts the full peer list on every change.
//!
//! Wire envelopes use a tagged JSON form `{ kind, payload }`. See the
//! "Wire protocol" section of `docs/dev/loom-multiuser.md` and the
//! `Incoming` / `Outgoing` enums below.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, RwLock};

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::response::IntoResponse;
use base64::Engine;
use futures_util::{SinkExt, StreamExt};
use prism_core::network::presence::types::PresenceState;
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;

use crate::auth::{decode_session_token, SESSION_SCOPE};
use crate::LoomRelayState;

/// Broadcast channel capacity per workspace. Generous enough to absorb
/// bursty CRDT edits without dropping; the receive side reports lag
/// rather than terminating.
const WS_BROADCAST_CAPACITY: usize = 1024;

// ── Wire envelopes ──────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", content = "payload", rename_all = "kebab-case")]
enum Incoming {
    Auth {
        token: String,
    },
    Subscribe {
        workspace: String,
    },
    Unsubscribe {
        workspace: String,
    },
    Update {
        workspace: String,
        /// Base64-encoded opaque CRDT bytes.
        bytes: String,
    },
    Presence {
        workspace: String,
        state: Box<PresenceState>,
    },
    /// Phase 7 — start a play session by uploading the workspace's
    /// current files. Server builds a `Bundle`, instantiates a
    /// `Playhead`, advances to the first choice / end, and broadcasts
    /// `play-state`.
    PlayStart {
        workspace: String,
        files: Vec<PlayFile>,
    },
    /// Phase 7 — advance the active session by index.
    /// Phase 4 (IDE redesign): optional `head` field, defaults to the
    /// session's primary head.
    PlayChoice {
        workspace: String,
        index: usize,
        head: Option<String>,
    },
    /// Phase 7 — tear the session down.
    PlayStop {
        workspace: String,
    },
    /// Phase 4 (IDE redesign) — branch a head. With `from_snapshot`,
    /// the new head starts from that captured state; otherwise it
    /// forks the live state of `parent` (or the primary head).
    PlayFork {
        workspace: String,
        parent: Option<String>,
        from_snapshot: Option<String>,
    },
    /// Phase 4 — capture the head's current state under an opaque id.
    PlaySnapshot {
        workspace: String,
        head: Option<String>,
        label: Option<String>,
    },
    /// Phase 4 — rewind a head to a captured snapshot.
    PlayRestore {
        workspace: String,
        head: String,
        snapshot: String,
    },
    /// Phase 4 — discard a non-primary head.
    PlayDropHead {
        workspace: String,
        head: String,
    },
    /// Phase 4 — change which head is the session's "primary" (the
    /// default for choose/snapshot when no explicit head is given).
    PlaySetPrimary {
        workspace: String,
        head: String,
    },
    /// Booth live-patch (spec §13.4) — skip the current beat on a
    /// head (defaults to primary).
    PlayBoothSkip {
        workspace: String,
        head: Option<String>,
    },
    /// Booth live-patch — inject a directive at the head of the
    /// playhead's queue. `raw` is the spec-form directive body, e.g.
    /// `sfx: thunder` or `set: Wren.health = 5`.
    PlayBoothForce {
        workspace: String,
        head: Option<String>,
        raw: String,
    },
    /// Booth live-patch — hot-reload the bundle on every head from
    /// freshly uploaded file sources.
    PlayBoothReload {
        workspace: String,
        files: Vec<PlayFile>,
    },
    Ping,
}

#[derive(Debug, Deserialize)]
struct PlayFile {
    path: String,
    source: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", content = "payload", rename_all = "kebab-case")]
enum Outgoing {
    AuthOk {
        subject: String,
    },
    Error {
        message: String,
    },
    Snapshot {
        workspace: String,
        bytes: String,
    },
    Update {
        workspace: String,
        bytes: String,
    },
    Presence {
        workspace: String,
        peers: Vec<PresenceState>,
    },
    /// Phase 7 — current state of a co-play session.
    PlayState(crate::play::PlayStateSnapshot),
    Pong,
}

impl Outgoing {
    fn to_ws(&self) -> Message {
        // `Outgoing` is hand-crafted, never user-input, so a serde
        // failure here would be a code bug — but we still fall back to
        // an `error` envelope instead of panicking on a hot path.
        match serde_json::to_string(self) {
            Ok(s) => Message::Text(s),
            Err(e) => Message::Text(
                serde_json::to_string(&Outgoing::Error {
                    message: format!("internal: serialize outgoing: {e}"),
                })
                .unwrap_or_else(|_| String::from("{\"kind\":\"error\"}")),
            ),
        }
    }
}

// ── Per-workspace hub ───────────────────────────────────────────────────────

/// One `WorkspaceHub` per live workspace. Carries the broadcast bus
/// used for `update` + `presence` fan-out plus the authoritative
/// presence map.
struct WorkspaceHub {
    tx: broadcast::Sender<WsBroadcast>,
    presence: RwLock<HashMap<String, PresenceState>>,
}

impl WorkspaceHub {
    fn new() -> Self {
        let (tx, _rx) = broadcast::channel(WS_BROADCAST_CAPACITY);
        Self {
            tx,
            presence: RwLock::new(HashMap::new()),
        }
    }
}

/// Broadcast payload. `from_peer` lets each connection's receive loop
/// skip messages it originated.
#[derive(Debug, Clone)]
struct WsBroadcast {
    from_peer: String,
    message: Outgoing,
}

/// Workspace-id → hub registry. Lives inside `LoomRelayState`.
#[derive(Default)]
pub struct WsHub {
    inner: RwLock<HashMap<String, Arc<WorkspaceHub>>>,
}

impl WsHub {
    pub fn new() -> Self {
        Self::default()
    }

    fn get_or_create(&self, workspace: &str) -> Arc<WorkspaceHub> {
        if let Some(hub) = self.inner.read().unwrap().get(workspace) {
            return hub.clone();
        }
        let mut w = self.inner.write().unwrap();
        w.entry(workspace.to_string())
            .or_insert_with(|| Arc::new(WorkspaceHub::new()))
            .clone()
    }
}

// ── HTTP upgrade handler ────────────────────────────────────────────────────

/// Axum handler — wires the `/ws` route in `build_router`.
pub async fn ws_handler(
    State(state): State<Arc<LoomRelayState>>,
    upgrade: WebSocketUpgrade,
) -> impl IntoResponse {
    upgrade.on_upgrade(move |socket| handle_socket(socket, state))
}

/// Authorization decision for a `subscribe` request. The session
/// token's subject is the canonical workspace owner; a capability
/// token whose `scope` matches the workspace id admits a share-link
/// guest under that token's subject.
fn authorize_subscribe(state: &LoomRelayState, auth: &AuthedSession, workspace: &str) -> bool {
    match &auth.kind {
        AuthKind::Session => state
            .workspaces
            .get(workspace)
            .map(|w| w.owner == auth.subject)
            .unwrap_or(false),
        AuthKind::Capability { scope } => {
            scope == workspace && state.workspaces.get(workspace).is_some()
        }
    }
}

#[derive(Clone, Debug)]
enum AuthKind {
    Session,
    Capability { scope: String },
}

#[derive(Clone, Debug)]
struct AuthedSession {
    subject: String,
    kind: AuthKind,
}

/// Parse a bearer-style token, trying the session-token path first
/// (which already enforces `scope == "session"` + expiry + signature)
/// and falling back to a raw capability-token decode + verify for
/// share-link guests.
fn authenticate(state: &LoomRelayState, token: &str) -> Result<AuthedSession, String> {
    let tokens = state.tokens();
    if let Ok(decoded) = decode_session_token(&tokens, token) {
        return Ok(AuthedSession {
            subject: decoded.subject,
            kind: AuthKind::Session,
        });
    }
    // Capability-token path. Decode the raw base64-url JSON, verify,
    // refuse anything still tagged with the session scope (those go
    // through `decode_session_token` above).
    let decoded = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(token)
        .map_err(|_| "malformed token".to_string())?;
    let parsed: prism_core::network::relay::modules::capability_tokens::CapabilityToken =
        serde_json::from_slice(&decoded).map_err(|_| "malformed token".to_string())?;
    tokens
        .verify(&parsed)
        .map_err(|_| "invalid token".to_string())?;
    if parsed.scope == SESSION_SCOPE {
        return Err("session token reached capability path".into());
    }
    Ok(AuthedSession {
        subject: parsed.subject,
        kind: AuthKind::Capability {
            scope: parsed.scope,
        },
    })
}

async fn handle_socket(socket: WebSocket, state: Arc<LoomRelayState>) {
    let peer_id = uuid::Uuid::new_v4().to_string();
    let (mut sink, mut stream) = socket.split();
    let mut auth: Option<AuthedSession> = None;
    // `workspace -> (hub, broadcast_task)`. We spawn one fan-out task
    // per subscription that forwards broadcast messages onto a single
    // mpsc into the shared sink.
    let mut subs: HashMap<String, Arc<WorkspaceHub>> = HashMap::new();
    let mut sub_tasks: HashMap<String, tokio::task::JoinHandle<()>> = HashMap::new();

    // mpsc into the sink so multiple sources (direct replies + every
    // per-subscription broadcast forwarder) can write without
    // contending on the sink itself.
    let (out_tx, mut out_rx) = tokio::sync::mpsc::unbounded_channel::<Message>();

    // Pump the mpsc into the sink.
    let sink_task = tokio::spawn(async move {
        while let Some(msg) = out_rx.recv().await {
            if sink.send(msg).await.is_err() {
                break;
            }
        }
        let _ = sink.close().await;
    });

    while let Some(frame) = stream.next().await {
        let msg = match frame {
            Ok(m) => m,
            Err(_) => break,
        };
        let text = match msg {
            Message::Text(t) => t,
            Message::Binary(_) => {
                let _ = out_tx.send(
                    Outgoing::Error {
                        message: "binary frames unsupported; use text envelopes".into(),
                    }
                    .to_ws(),
                );
                continue;
            }
            Message::Ping(p) => {
                let _ = out_tx.send(Message::Pong(p));
                continue;
            }
            Message::Pong(_) => continue,
            Message::Close(_) => break,
        };

        let parsed: Incoming = match serde_json::from_str(&text) {
            Ok(v) => v,
            Err(e) => {
                let _ = out_tx.send(
                    Outgoing::Error {
                        message: format!("malformed envelope: {e}"),
                    }
                    .to_ws(),
                );
                continue;
            }
        };

        match parsed {
            Incoming::Auth { token } => match authenticate(&state, &token) {
                Ok(session) => {
                    let subject = session.subject.clone();
                    auth = Some(session);
                    let _ = out_tx.send(Outgoing::AuthOk { subject }.to_ws());
                }
                Err(message) => {
                    let _ = out_tx.send(Outgoing::Error { message }.to_ws());
                }
            },
            Incoming::Ping => {
                let _ = out_tx.send(Outgoing::Pong.to_ws());
            }
            Incoming::Subscribe { workspace } => {
                let Some(session) = auth.as_ref() else {
                    let _ = out_tx.send(
                        Outgoing::Error {
                            message: "subscribe before auth".into(),
                        }
                        .to_ws(),
                    );
                    continue;
                };
                if !authorize_subscribe(&state, session, &workspace) {
                    let _ = out_tx.send(
                        Outgoing::Error {
                            message: "forbidden: workspace ACL".into(),
                        }
                        .to_ws(),
                    );
                    continue;
                }
                if subs.contains_key(&workspace) {
                    continue;
                }
                let hub = state.ws_hub.get_or_create(&workspace);
                // Snapshot first — the client expects exactly one
                // `snapshot` envelope per successful subscribe.
                let bytes = state
                    .collections()
                    .export_snapshot(&workspace)
                    .unwrap_or_default();
                let encoded = base64::engine::general_purpose::STANDARD.encode(bytes);
                let _ = out_tx.send(
                    Outgoing::Snapshot {
                        workspace: workspace.clone(),
                        bytes: encoded,
                    }
                    .to_ws(),
                );
                // Spawn the fan-out task — receives broadcasts for
                // this workspace and forwards everything not from
                // this peer into the shared sink.
                let mut rx = hub.tx.subscribe();
                let out_tx_task = out_tx.clone();
                let peer_id_task = peer_id.clone();
                let task = tokio::spawn(async move {
                    loop {
                        match rx.recv().await {
                            Ok(bcast) => {
                                if bcast.from_peer == peer_id_task {
                                    continue;
                                }
                                if out_tx_task.send(bcast.message.to_ws()).is_err() {
                                    break;
                                }
                            }
                            Err(broadcast::error::RecvError::Lagged(_)) => continue,
                            Err(broadcast::error::RecvError::Closed) => break,
                        }
                    }
                });
                // Phase 4 / loose ends: if a play session is already
                // running on this workspace, hand the new subscriber
                // its current snapshot so the runner panels populate
                // immediately. Without this they'd sit empty until
                // somebody on the workspace took the next action.
                if let Some(snap) = state.play.snapshot(&workspace) {
                    let _ = out_tx.send(Outgoing::PlayState(snap).to_ws());
                }
                subs.insert(workspace.clone(), hub);
                sub_tasks.insert(workspace, task);
            }
            Incoming::Unsubscribe { workspace } => {
                if let Some(task) = sub_tasks.remove(&workspace) {
                    task.abort();
                }
                if let Some(hub) = subs.remove(&workspace) {
                    drop_peer_presence(&hub, &peer_id, &workspace);
                }
            }
            Incoming::Update { workspace, bytes } => {
                if auth.is_none() {
                    let _ = out_tx.send(
                        Outgoing::Error {
                            message: "update before auth".into(),
                        }
                        .to_ws(),
                    );
                    continue;
                }
                let Some(hub) = subs.get(&workspace).cloned() else {
                    let _ = out_tx.send(
                        Outgoing::Error {
                            message: format!("update for unsubscribed workspace {workspace}"),
                        }
                        .to_ws(),
                    );
                    continue;
                };
                let raw = match base64::engine::general_purpose::STANDARD.decode(&bytes) {
                    Ok(v) => v,
                    Err(e) => {
                        let _ = out_tx.send(
                            Outgoing::Error {
                                message: format!("update bytes not base64: {e}"),
                            }
                            .to_ws(),
                        );
                        continue;
                    }
                };
                let now = chrono::Utc::now().to_rfc3339();
                state.collections().import_snapshot(&workspace, raw, &now);
                let _ = hub.tx.send(WsBroadcast {
                    from_peer: peer_id.clone(),
                    message: Outgoing::Update { workspace, bytes },
                });
            }
            Incoming::Presence {
                workspace,
                state: pstate,
            } => {
                if auth.is_none() {
                    let _ = out_tx.send(
                        Outgoing::Error {
                            message: "presence before auth".into(),
                        }
                        .to_ws(),
                    );
                    continue;
                }
                let Some(hub) = subs.get(&workspace).cloned() else {
                    let _ = out_tx.send(
                        Outgoing::Error {
                            message: format!("presence for unsubscribed workspace {workspace}"),
                        }
                        .to_ws(),
                    );
                    continue;
                };
                {
                    let mut guard = hub.presence.write().unwrap();
                    guard.insert(peer_id.clone(), *pstate);
                }
                let peers = hub
                    .presence
                    .read()
                    .unwrap()
                    .values()
                    .cloned()
                    .collect::<Vec<_>>();
                let msg = Outgoing::Presence {
                    workspace: workspace.clone(),
                    peers,
                };
                // Echo to the originator too — they want their own
                // entry reflected in the canonical peer list.
                let _ = out_tx.send(msg.to_ws());
                let _ = hub.tx.send(WsBroadcast {
                    from_peer: peer_id.clone(),
                    message: msg,
                });
            }
            Incoming::PlayStart { workspace, files } => {
                let Some(session) = auth.as_ref() else {
                    let _ = out_tx.send(
                        Outgoing::Error {
                            message: "play-start before auth".into(),
                        }
                        .to_ws(),
                    );
                    continue;
                };
                let Some(hub) = subs.get(&workspace).cloned() else {
                    let _ = out_tx.send(
                        Outgoing::Error {
                            message: format!("play-start for unsubscribed workspace {workspace}"),
                        }
                        .to_ws(),
                    );
                    continue;
                };
                let sources: Vec<(String, String)> =
                    files.into_iter().map(|f| (f.path, f.source)).collect();
                match state
                    .play
                    .start(workspace.clone(), sources, session.subject.clone())
                {
                    Ok(snap) => {
                        let msg = Outgoing::PlayState(snap);
                        let _ = out_tx.send(msg.to_ws());
                        let _ = hub.tx.send(WsBroadcast {
                            from_peer: peer_id.clone(),
                            message: msg,
                        });
                    }
                    Err(e) => {
                        let _ = out_tx.send(
                            Outgoing::Error {
                                message: format!("play-start: {e}"),
                            }
                            .to_ws(),
                        );
                    }
                }
            }
            Incoming::PlayChoice {
                workspace,
                index,
                head,
            } => {
                if !require_authed_subscribed(&auth, &subs, &workspace, "play-choice", &out_tx) {
                    continue;
                }
                let hub = subs.get(&workspace).cloned().unwrap();
                match state.play.choose(&workspace, head.as_deref(), index) {
                    Ok(snap) => broadcast_play_state(snap, &out_tx, &hub.tx, &peer_id),
                    Err(e) => send_error(&out_tx, format!("play-choice: {e}")),
                }
            }
            Incoming::PlayStop { workspace } => {
                state.play.stop(&workspace);
            }
            Incoming::PlayFork {
                workspace,
                parent,
                from_snapshot,
            } => {
                if !require_authed_subscribed(&auth, &subs, &workspace, "play-fork", &out_tx) {
                    continue;
                }
                let hub = subs.get(&workspace).cloned().unwrap();
                match state
                    .play
                    .fork(&workspace, parent.as_deref(), from_snapshot.as_deref())
                {
                    Ok((snap, _new_head)) => {
                        broadcast_play_state(snap, &out_tx, &hub.tx, &peer_id);
                    }
                    Err(e) => send_error(&out_tx, format!("play-fork: {e}")),
                }
            }
            Incoming::PlaySnapshot {
                workspace,
                head,
                label,
            } => {
                if !require_authed_subscribed(&auth, &subs, &workspace, "play-snapshot", &out_tx) {
                    continue;
                }
                let hub = subs.get(&workspace).cloned().unwrap();
                match state.play.snapshot_head(&workspace, head.as_deref(), label) {
                    Ok((snap, _snap_id)) => {
                        broadcast_play_state(snap, &out_tx, &hub.tx, &peer_id);
                    }
                    Err(e) => send_error(&out_tx, format!("play-snapshot: {e}")),
                }
            }
            Incoming::PlayRestore {
                workspace,
                head,
                snapshot,
            } => {
                if !require_authed_subscribed(&auth, &subs, &workspace, "play-restore", &out_tx) {
                    continue;
                }
                let hub = subs.get(&workspace).cloned().unwrap();
                match state.play.restore(&workspace, &head, &snapshot) {
                    Ok(snap) => broadcast_play_state(snap, &out_tx, &hub.tx, &peer_id),
                    Err(e) => send_error(&out_tx, format!("play-restore: {e}")),
                }
            }
            Incoming::PlayDropHead { workspace, head } => {
                if !require_authed_subscribed(&auth, &subs, &workspace, "play-drop-head", &out_tx) {
                    continue;
                }
                let hub = subs.get(&workspace).cloned().unwrap();
                match state.play.drop_head(&workspace, &head) {
                    Ok(snap) => broadcast_play_state(snap, &out_tx, &hub.tx, &peer_id),
                    Err(e) => send_error(&out_tx, format!("play-drop-head: {e}")),
                }
            }
            Incoming::PlaySetPrimary { workspace, head } => {
                if !require_authed_subscribed(&auth, &subs, &workspace, "play-set-primary", &out_tx)
                {
                    continue;
                }
                let hub = subs.get(&workspace).cloned().unwrap();
                match state.play.set_primary(&workspace, &head) {
                    Ok(snap) => broadcast_play_state(snap, &out_tx, &hub.tx, &peer_id),
                    Err(e) => send_error(&out_tx, format!("play-set-primary: {e}")),
                }
            }
            Incoming::PlayBoothSkip { workspace, head } => {
                if !require_authed_subscribed(&auth, &subs, &workspace, "play-booth-skip", &out_tx)
                {
                    continue;
                }
                let hub = subs.get(&workspace).cloned().unwrap();
                match state.play.booth_skip(&workspace, head.as_deref()) {
                    Ok(snap) => broadcast_play_state(snap, &out_tx, &hub.tx, &peer_id),
                    Err(e) => send_error(&out_tx, format!("play-booth-skip: {e}")),
                }
            }
            Incoming::PlayBoothForce {
                workspace,
                head,
                raw,
            } => {
                if !require_authed_subscribed(&auth, &subs, &workspace, "play-booth-force", &out_tx)
                {
                    continue;
                }
                let hub = subs.get(&workspace).cloned().unwrap();
                match state.play.booth_force(&workspace, head.as_deref(), raw) {
                    Ok(snap) => broadcast_play_state(snap, &out_tx, &hub.tx, &peer_id),
                    Err(e) => send_error(&out_tx, format!("play-booth-force: {e}")),
                }
            }
            Incoming::PlayBoothReload { workspace, files } => {
                if !require_authed_subscribed(
                    &auth,
                    &subs,
                    &workspace,
                    "play-booth-reload",
                    &out_tx,
                ) {
                    continue;
                }
                let hub = subs.get(&workspace).cloned().unwrap();
                let sources: Vec<(String, String)> =
                    files.into_iter().map(|f| (f.path, f.source)).collect();
                match state.play.booth_hot_reload(&workspace, sources) {
                    Ok(snap) => broadcast_play_state(snap, &out_tx, &hub.tx, &peer_id),
                    Err(e) => send_error(&out_tx, format!("play-booth-reload: {e}")),
                }
            }
        }
    }

    // Cleanup: tear down every per-subscription task + drop our
    // presence entry from every workspace we joined.
    let joined: HashSet<String> = subs.keys().cloned().collect();
    for (_, task) in sub_tasks.drain() {
        task.abort();
    }
    for workspace in &joined {
        if let Some(hub) = subs.get(workspace) {
            drop_peer_presence(hub, &peer_id, workspace);
        }
    }
    drop(out_tx);
    let _ = sink_task.await;
}

/// Phase 4 helper: precondition shared by every play-* command.
/// Sends an `error` envelope to the caller and returns false when the
/// connection is unauthed or not subscribed to the workspace.
fn require_authed_subscribed(
    auth: &Option<AuthedSession>,
    subs: &HashMap<String, Arc<WorkspaceHub>>,
    workspace: &str,
    cmd: &str,
    out_tx: &tokio::sync::mpsc::UnboundedSender<Message>,
) -> bool {
    if auth.is_none() {
        send_error(out_tx, format!("{cmd} before auth"));
        return false;
    }
    if !subs.contains_key(workspace) {
        send_error(
            out_tx,
            format!("{cmd} for unsubscribed workspace {workspace}"),
        );
        return false;
    }
    true
}

fn send_error(out_tx: &tokio::sync::mpsc::UnboundedSender<Message>, message: String) {
    let _ = out_tx.send(Outgoing::Error { message }.to_ws());
}

/// Send a `play-state` snapshot to the originator AND fan it out to
/// every other subscriber on the workspace.
fn broadcast_play_state(
    snap: crate::play::PlayStateSnapshot,
    out_tx: &tokio::sync::mpsc::UnboundedSender<Message>,
    hub_tx: &tokio::sync::broadcast::Sender<WsBroadcast>,
    peer_id: &str,
) {
    let msg = Outgoing::PlayState(snap);
    let _ = out_tx.send(msg.to_ws());
    let _ = hub_tx.send(WsBroadcast {
        from_peer: peer_id.to_string(),
        message: msg,
    });
}

/// Remove the given peer's presence entry and broadcast the
/// updated list. Called on unsubscribe + on disconnect so other
/// peers see departures promptly.
fn drop_peer_presence(hub: &Arc<WorkspaceHub>, peer_id: &str, workspace: &str) {
    let removed = {
        let mut guard = hub.presence.write().unwrap();
        guard.remove(peer_id).is_some()
    };
    if !removed {
        return;
    }
    let peers = hub
        .presence
        .read()
        .unwrap()
        .values()
        .cloned()
        .collect::<Vec<_>>();
    let _ = hub.tx.send(WsBroadcast {
        from_peer: peer_id.to_string(),
        message: Outgoing::Presence {
            workspace: workspace.to_string(),
            peers,
        },
    });
}
