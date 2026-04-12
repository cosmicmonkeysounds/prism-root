//! Conferencing module — WebRTC peer connections + data channels owned by
//! the daemon, exposed through eleven `conferencing.*` commands.
//!
//! | Command                                | Payload                                                        | Result                                  |
//! |----------------------------------------|----------------------------------------------------------------|-----------------------------------------|
//! | `conferencing.create_peer`             | `{ name?, ice_servers?: [string] }`                            | `{ id }`                                |
//! | `conferencing.create_data_channel`     | `{ id, label, ordered? }`                                      | `{}`                                    |
//! | `conferencing.create_offer`            | `{ id }`                                                       | `{ sdp, type }`                         |
//! | `conferencing.create_answer`           | `{ id }`                                                       | `{ sdp, type }`                         |
//! | `conferencing.set_local_description`   | `{ id, sdp, type }`                                            | `{}`                                    |
//! | `conferencing.set_remote_description`  | `{ id, sdp, type }`                                            | `{}`                                    |
//! | `conferencing.local_description`       | `{ id }`                                                       | `{ sdp, type } \| null`                 |
//! | `conferencing.add_ice_candidate`       | `{ id, candidate, sdpMid?, sdpMLineIndex? }`                   | `{}`                                    |
//! | `conferencing.send_data`               | `{ id, label, message, binary? }`                              | `{}`                                    |
//! | `conferencing.recv_data`               | `{ id, max?: usize }`                                          | `{ messages: [{label, text?, bytes?}] }`|
//! | `conferencing.peer_state`              | `{ id }`                                                       | `{ signaling, ice, peer }`              |
//! | `conferencing.list_peers`              | `{}`                                                           | `{ peers: [...] }`                      |
//! | `conferencing.close_peer`              | `{ id }`                                                       | `{ closed: bool }`                      |
//!
//! ## Architecture
//!
//! WebRTC is the SPEC's chosen A/V transport (P2P data + audio + video, with
//! an SFU fallback at the Relay layer). Putting it inside the daemon means
//! every Prism surface — Tauri desktop, mobile FFI, browser WASM (eventually
//! via the C ABI), CLI — can join calls through one shared command surface
//! without each shell having to embed its own WebRTC stack.
//!
//! The [`webrtc`](https://crates.io/crates/webrtc) crate is fully async and
//! tokio-driven. The daemon kernel itself is sync — every other module
//! operates inside the synchronous `kernel.invoke` boundary — so the
//! [`ConferencingManager`] owns its own multi-threaded tokio runtime and
//! every command handler does a single `runtime.block_on` to dispatch the
//! corresponding async work. This keeps the existing transport layers
//! untouched: a Tauri command, a CLI stdio frame, a UniFFI bridge, all hit
//! the same blocking entry point and the runtime is invisible to them.
//!
//! Each peer connection exposes:
//!   * a thread-safe `Mailbox` (the same primitive [`actors_module`] uses)
//!     into which inbound data-channel messages are pushed by the on_message
//!     callback registered when the data channel opens, and out of which
//!     `conferencing.recv_data` drains in batches;
//!   * a `Mutex<HashMap<String, Arc<RTCDataChannel>>>` of every data channel
//!     keyed by label (both locally created and the ones surfaced by the
//!     remote via `on_data_channel`), so `conferencing.send_data` can pick
//!     the channel by name without the caller tracking SCTP stream ids.
//!
//! The module is feature-gated as `conferencing` and is desktop-only — the
//! `webrtc` crate pulls in tokio plus a network stack that mobile/wasm/embedded
//! cannot link against. Mobile builds can still join calls by talking to a
//! desktop daemon over the same `kernel.invoke` interface; the binary just
//! lives elsewhere.

use crate::builder::DaemonBuilder;
use crate::module::DaemonModule;
use crate::registry::CommandError;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value as JsonValue};
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use bytes::Bytes;
use tokio::runtime::Runtime;
use webrtc::api::media_engine::MediaEngine;
use webrtc::api::APIBuilder;
use webrtc::data_channel::data_channel_init::RTCDataChannelInit;
use webrtc::data_channel::data_channel_message::DataChannelMessage;
use webrtc::data_channel::RTCDataChannel;
use webrtc::ice_transport::ice_candidate::RTCIceCandidateInit;
use webrtc::ice_transport::ice_server::RTCIceServer;
use webrtc::peer_connection::configuration::RTCConfiguration;
use webrtc::peer_connection::peer_connection_state::RTCPeerConnectionState;
use webrtc::peer_connection::sdp::sdp_type::RTCSdpType;
use webrtc::peer_connection::sdp::session_description::RTCSessionDescription;
use webrtc::peer_connection::RTCPeerConnection;

/// Manager for every WebRTC peer connection the daemon currently owns.
pub struct ConferencingManager {
    runtime: Arc<Runtime>,
    next_id: AtomicU64,
    peers: Mutex<HashMap<u64, Arc<PeerEntry>>>,
}

/// All the per-peer state we need to drive the command surface from a sync
/// caller. The `Arc<RTCPeerConnection>` is the actual webrtc handle; the
/// other fields hold the inbound message buffer and the data-channel index.
struct PeerEntry {
    name: String,
    pc: Arc<RTCPeerConnection>,
    inbox: Arc<DataInbox>,
    channels: Arc<Mutex<HashMap<String, Arc<RTCDataChannel>>>>,
}

/// Inbound mailbox for data-channel messages, drained by
/// `conferencing.recv_data`. Storing the channel label alongside the bytes
/// keeps multi-channel calls debuggable from the host.
#[derive(Default)]
struct DataInbox {
    queue: Mutex<VecDeque<InboundDataMessage>>,
}

/// One inbound data-channel frame: which labelled channel it arrived on,
/// whether the wire framing was text or binary, and the raw bytes.
#[derive(Debug, Clone)]
pub struct InboundDataMessage {
    pub label: String,
    pub is_string: bool,
    pub data: Bytes,
}

impl DataInbox {
    fn push(&self, msg: InboundDataMessage) {
        if let Ok(mut q) = self.queue.lock() {
            q.push_back(msg);
        }
    }

    fn drain(&self, max: usize) -> Vec<InboundDataMessage> {
        let mut q = match self.queue.lock() {
            Ok(q) => q,
            Err(_) => return Vec::new(),
        };
        let n = q.len().min(max);
        q.drain(..n).collect()
    }
}

impl Default for ConferencingManager {
    fn default() -> Self {
        Self::new()
    }
}

impl ConferencingManager {
    /// Allocate a fresh manager backed by a multi-threaded tokio runtime.
    /// The runtime lives as long as the manager and is shared across every
    /// peer it spawns.
    pub fn new() -> Self {
        let runtime = Runtime::new().expect("conferencing: failed to spawn tokio runtime");
        Self {
            runtime: Arc::new(runtime),
            next_id: AtomicU64::new(1),
            peers: Mutex::new(HashMap::new()),
        }
    }

    /// Allocate a new peer connection. The optional `ice_servers` list is
    /// passed straight through to the underlying [`RTCConfiguration`] —
    /// supply STUN/TURN URIs as `["stun:stun.l.google.com:19302"]` etc.
    pub fn create_peer(&self, name: String, ice_servers: Vec<String>) -> Result<u64, String> {
        let mut media_engine = MediaEngine::default();
        media_engine
            .register_default_codecs()
            .map_err(|e| format!("media_engine.register_default_codecs: {e}"))?;
        let api = APIBuilder::new().with_media_engine(media_engine).build();

        let config = RTCConfiguration {
            ice_servers: if ice_servers.is_empty() {
                Vec::new()
            } else {
                vec![RTCIceServer {
                    urls: ice_servers,
                    ..Default::default()
                }]
            },
            ..Default::default()
        };

        let pc = self
            .runtime
            .block_on(async { api.new_peer_connection(config).await })
            .map_err(|e| format!("new_peer_connection: {e}"))?;
        let pc = Arc::new(pc);

        let inbox = Arc::new(DataInbox::default());
        let channels: Arc<Mutex<HashMap<String, Arc<RTCDataChannel>>>> =
            Arc::new(Mutex::new(HashMap::new()));

        // When the remote opens a data channel, surface it through the same
        // inbox + channel index so the host can recv/send by label.
        let inbox_for_dc = inbox.clone();
        let channels_for_dc = channels.clone();
        pc.on_data_channel(Box::new(move |dc: Arc<RTCDataChannel>| {
            let label = dc.label().to_string();
            channels_for_dc
                .lock()
                .ok()
                .map(|mut g| g.insert(label.clone(), dc.clone()));
            wire_inbox(&dc, label, inbox_for_dc.clone());
            Box::pin(async {})
        }));

        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let entry = Arc::new(PeerEntry {
            name,
            pc,
            inbox,
            channels,
        });
        self.peers
            .lock()
            .map_err(|_| "peers map poisoned".to_string())?
            .insert(id, entry);
        Ok(id)
    }

    /// Create a new data channel on the local peer. Stored in the channel
    /// index under `label` so subsequent `send_data` calls can address it
    /// by name.
    pub fn create_data_channel(
        &self,
        id: u64,
        label: String,
        ordered: Option<bool>,
    ) -> Result<(), String> {
        let entry = self.entry(id)?;
        let pc = entry.pc.clone();
        let inbox = entry.inbox.clone();
        let channels = entry.channels.clone();
        let label_for_open = label.clone();
        self.runtime
            .block_on(async move {
                let init = RTCDataChannelInit {
                    ordered,
                    ..Default::default()
                };
                let dc = pc.create_data_channel(&label_for_open, Some(init)).await?;
                if let Ok(mut g) = channels.lock() {
                    g.insert(label_for_open.clone(), dc.clone());
                }
                wire_inbox(&dc, label_for_open, inbox);
                Ok::<(), webrtc::Error>(())
            })
            .map_err(|e| format!("create_data_channel: {e}"))
    }

    /// Generate an offer SDP for the local peer.
    pub fn create_offer(&self, id: u64) -> Result<RTCSessionDescription, String> {
        let entry = self.entry(id)?;
        let pc = entry.pc.clone();
        self.runtime
            .block_on(async move { pc.create_offer(None).await })
            .map_err(|e| format!("create_offer: {e}"))
    }

    /// Generate an answer SDP for the local peer (the remote offer must
    /// have been set first).
    pub fn create_answer(&self, id: u64) -> Result<RTCSessionDescription, String> {
        let entry = self.entry(id)?;
        let pc = entry.pc.clone();
        self.runtime
            .block_on(async move { pc.create_answer(None).await })
            .map_err(|e| format!("create_answer: {e}"))
    }

    /// Apply an SDP as the local description.
    pub fn set_local_description(
        &self,
        id: u64,
        desc: RTCSessionDescription,
    ) -> Result<(), String> {
        let entry = self.entry(id)?;
        let pc = entry.pc.clone();
        self.runtime
            .block_on(async move { pc.set_local_description(desc).await })
            .map_err(|e| format!("set_local_description: {e}"))
    }

    /// Apply an SDP as the remote description.
    pub fn set_remote_description(
        &self,
        id: u64,
        desc: RTCSessionDescription,
    ) -> Result<(), String> {
        let entry = self.entry(id)?;
        let pc = entry.pc.clone();
        self.runtime
            .block_on(async move { pc.set_remote_description(desc).await })
            .map_err(|e| format!("set_remote_description: {e}"))
    }

    /// Snapshot of the local description.
    pub fn local_description(&self, id: u64) -> Result<Option<RTCSessionDescription>, String> {
        let entry = self.entry(id)?;
        let pc = entry.pc.clone();
        Ok(self
            .runtime
            .block_on(async move { pc.local_description().await }))
    }

    /// Apply a remote ICE candidate.
    pub fn add_ice_candidate(&self, id: u64, candidate: RTCIceCandidateInit) -> Result<(), String> {
        let entry = self.entry(id)?;
        let pc = entry.pc.clone();
        self.runtime
            .block_on(async move { pc.add_ice_candidate(candidate).await })
            .map_err(|e| format!("add_ice_candidate: {e}"))
    }

    /// Send bytes (or a string) on the named data channel. The channel must
    /// already exist — either created locally with `create_data_channel` or
    /// surfaced by the remote via `on_data_channel`.
    pub fn send_data(
        &self,
        id: u64,
        label: &str,
        payload: Bytes,
        is_string: bool,
    ) -> Result<(), String> {
        let entry = self.entry(id)?;
        let dc = entry
            .channels
            .lock()
            .map_err(|_| "channels map poisoned".to_string())?
            .get(label)
            .cloned()
            .ok_or_else(|| format!("unknown data channel: {label}"))?;
        self.runtime
            .block_on(async move {
                if is_string {
                    let s = std::str::from_utf8(&payload)
                        .map_err(|e| webrtc::Error::new(format!("non-utf8 string payload: {e}")))?;
                    dc.send_text(s.to_string()).await?;
                } else {
                    dc.send(&payload).await?;
                }
                Ok::<(), webrtc::Error>(())
            })
            .map(|_| ())
            .map_err(|e| format!("send_data: {e}"))
    }

    /// Drain up to `max` queued inbound data-channel messages.
    pub fn recv_data(&self, id: u64, max: usize) -> Result<Vec<InboundDataMessage>, String> {
        let entry = self.entry(id)?;
        Ok(entry.inbox.drain(max))
    }

    /// Snapshot of the peer's signaling/ICE/connection state.
    pub fn peer_state(&self, id: u64) -> Result<PeerStateSnapshot, String> {
        let entry = self.entry(id)?;
        Ok(PeerStateSnapshot {
            id,
            name: entry.name.clone(),
            signaling: format!("{}", entry.pc.signaling_state()),
            ice: format!("{}", entry.pc.ice_connection_state()),
            peer: peer_state_label(entry.pc.connection_state()),
        })
    }

    /// List every peer the manager currently owns, sorted by id.
    pub fn list_peers(&self) -> Vec<PeerStateSnapshot> {
        let map = match self.peers.lock() {
            Ok(g) => g,
            Err(_) => return Vec::new(),
        };
        let mut out: Vec<PeerStateSnapshot> = map
            .iter()
            .map(|(id, entry)| PeerStateSnapshot {
                id: *id,
                name: entry.name.clone(),
                signaling: format!("{}", entry.pc.signaling_state()),
                ice: format!("{}", entry.pc.ice_connection_state()),
                peer: peer_state_label(entry.pc.connection_state()),
            })
            .collect();
        out.sort_by_key(|s| s.id);
        out
    }

    /// Close a peer connection and drop it from the manager. Idempotent.
    pub fn close_peer(&self, id: u64) -> Result<bool, String> {
        let entry = {
            let mut map = self
                .peers
                .lock()
                .map_err(|_| "peers map poisoned".to_string())?;
            match map.remove(&id) {
                Some(e) => e,
                None => return Ok(false),
            }
        };
        let pc = entry.pc.clone();
        let _ = self.runtime.block_on(async move { pc.close().await });
        Ok(true)
    }

    /// Close every peer. Used by `dispose()` and tests.
    pub fn close_all(&self) {
        let ids: Vec<u64> = self
            .peers
            .lock()
            .map(|m| m.keys().copied().collect())
            .unwrap_or_default();
        for id in ids {
            let _ = self.close_peer(id);
        }
    }

    fn entry(&self, id: u64) -> Result<Arc<PeerEntry>, String> {
        let map = self
            .peers
            .lock()
            .map_err(|_| "peers map poisoned".to_string())?;
        map.get(&id)
            .cloned()
            .ok_or_else(|| format!("unknown peer: {id}"))
    }
}

impl Drop for ConferencingManager {
    fn drop(&mut self) {
        self.close_all();
    }
}

/// Wire an `on_message` callback that pushes incoming data-channel messages
/// onto the shared inbox.
fn wire_inbox(dc: &Arc<RTCDataChannel>, label: String, inbox: Arc<DataInbox>) {
    let inbox_for_msg = inbox.clone();
    let label_for_msg = label.clone();
    dc.on_message(Box::new(move |msg: DataChannelMessage| {
        let inbox = inbox_for_msg.clone();
        let label = label_for_msg.clone();
        Box::pin(async move {
            inbox.push(InboundDataMessage {
                label,
                is_string: msg.is_string,
                data: msg.data,
            });
        })
    }));
}

fn peer_state_label(state: RTCPeerConnectionState) -> String {
    format!("{state}")
}

/// Public snapshot returned by `conferencing.peer_state` and
/// `conferencing.list_peers`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerStateSnapshot {
    pub id: u64,
    pub name: String,
    pub signaling: String,
    pub ice: String,
    pub peer: String,
}

// ── Module wiring ──────────────────────────────────────────────────────

/// The built-in conferencing module. Stateless — the state lives on the
/// shared [`ConferencingManager`] stashed on the builder.
pub struct ConferencingModule;

impl DaemonModule for ConferencingModule {
    fn id(&self) -> &str {
        "prism.conferencing"
    }

    fn install(&self, builder: &mut DaemonBuilder) -> Result<(), CommandError> {
        let manager = builder
            .conferencing_manager_slot()
            .get_or_insert_with(|| Arc::new(ConferencingManager::new()))
            .clone();
        let registry = builder.registry().clone();

        let m = manager.clone();
        registry.register("conferencing.create_peer", move |payload| {
            let args: CreatePeerArgs = parse(payload, "conferencing.create_peer")?;
            let id = m
                .create_peer(
                    args.name.unwrap_or_else(|| "peer".to_string()),
                    args.ice_servers.unwrap_or_default(),
                )
                .map_err(|e| CommandError::handler("conferencing.create_peer", e))?;
            Ok(json!({ "id": id }))
        })?;

        let m = manager.clone();
        registry.register("conferencing.create_data_channel", move |payload| {
            let args: CreateDataChannelArgs = parse(payload, "conferencing.create_data_channel")?;
            m.create_data_channel(args.id, args.label, args.ordered)
                .map_err(|e| CommandError::handler("conferencing.create_data_channel", e))?;
            Ok(json!({}))
        })?;

        let m = manager.clone();
        registry.register("conferencing.create_offer", move |payload| {
            let args: IdArgs = parse(payload, "conferencing.create_offer")?;
            let desc = m
                .create_offer(args.id)
                .map_err(|e| CommandError::handler("conferencing.create_offer", e))?;
            Ok(session_description_to_json(&desc))
        })?;

        let m = manager.clone();
        registry.register("conferencing.create_answer", move |payload| {
            let args: IdArgs = parse(payload, "conferencing.create_answer")?;
            let desc = m
                .create_answer(args.id)
                .map_err(|e| CommandError::handler("conferencing.create_answer", e))?;
            Ok(session_description_to_json(&desc))
        })?;

        let m = manager.clone();
        registry.register("conferencing.set_local_description", move |payload| {
            let args: DescriptionArgs = parse(payload, "conferencing.set_local_description")?;
            let desc = description_from_args(&args)
                .map_err(|e| CommandError::handler("conferencing.set_local_description", e))?;
            m.set_local_description(args.id, desc)
                .map_err(|e| CommandError::handler("conferencing.set_local_description", e))?;
            Ok(json!({}))
        })?;

        let m = manager.clone();
        registry.register("conferencing.set_remote_description", move |payload| {
            let args: DescriptionArgs = parse(payload, "conferencing.set_remote_description")?;
            let desc = description_from_args(&args)
                .map_err(|e| CommandError::handler("conferencing.set_remote_description", e))?;
            m.set_remote_description(args.id, desc)
                .map_err(|e| CommandError::handler("conferencing.set_remote_description", e))?;
            Ok(json!({}))
        })?;

        let m = manager.clone();
        registry.register("conferencing.local_description", move |payload| {
            let args: IdArgs = parse(payload, "conferencing.local_description")?;
            let desc = m
                .local_description(args.id)
                .map_err(|e| CommandError::handler("conferencing.local_description", e))?;
            Ok(match desc {
                Some(d) => session_description_to_json(&d),
                None => JsonValue::Null,
            })
        })?;

        let m = manager.clone();
        registry.register("conferencing.add_ice_candidate", move |payload| {
            let args: IceCandidateArgs = parse(payload, "conferencing.add_ice_candidate")?;
            let candidate = RTCIceCandidateInit {
                candidate: args.candidate,
                sdp_mid: args.sdp_mid,
                sdp_mline_index: args.sdp_mline_index,
                username_fragment: args.username_fragment,
            };
            m.add_ice_candidate(args.id, candidate)
                .map_err(|e| CommandError::handler("conferencing.add_ice_candidate", e))?;
            Ok(json!({}))
        })?;

        let m = manager.clone();
        registry.register("conferencing.send_data", move |payload| {
            let args: SendDataArgs = parse(payload, "conferencing.send_data")?;
            let (bytes, is_string) = match (args.message, args.binary) {
                (Some(text), None) => (Bytes::from(text.into_bytes()), true),
                (None, Some(hex_bytes)) => {
                    let raw = hex::decode(&hex_bytes).map_err(|e| {
                        CommandError::handler(
                            "conferencing.send_data",
                            format!("binary must be hex: {e}"),
                        )
                    })?;
                    (Bytes::from(raw), false)
                }
                _ => {
                    return Err(CommandError::handler(
                        "conferencing.send_data",
                        "exactly one of `message` (text) or `binary` (hex bytes) is required",
                    ))
                }
            };
            m.send_data(args.id, &args.label, bytes, is_string)
                .map_err(|e| CommandError::handler("conferencing.send_data", e))?;
            Ok(json!({}))
        })?;

        let m = manager.clone();
        registry.register("conferencing.recv_data", move |payload| {
            let args: RecvDataArgs = parse(payload, "conferencing.recv_data")?;
            let max = args.max.unwrap_or(64);
            let messages = m
                .recv_data(args.id, max)
                .map_err(|e| CommandError::handler("conferencing.recv_data", e))?;
            let json_msgs: Vec<JsonValue> = messages
                .into_iter()
                .map(|msg| {
                    if msg.is_string {
                        json!({
                            "label": msg.label,
                            "text": String::from_utf8_lossy(&msg.data),
                        })
                    } else {
                        json!({
                            "label": msg.label,
                            "bytes": hex::encode(&msg.data),
                        })
                    }
                })
                .collect();
            Ok(json!({ "messages": json_msgs }))
        })?;

        let m = manager.clone();
        registry.register("conferencing.peer_state", move |payload| {
            let args: IdArgs = parse(payload, "conferencing.peer_state")?;
            let state = m
                .peer_state(args.id)
                .map_err(|e| CommandError::handler("conferencing.peer_state", e))?;
            serde_json::to_value(state)
                .map_err(|e| CommandError::handler("conferencing.peer_state", e.to_string()))
        })?;

        let m = manager.clone();
        registry.register("conferencing.list_peers", move |_payload| {
            Ok(json!({ "peers": m.list_peers() }))
        })?;

        let m = manager;
        registry.register("conferencing.close_peer", move |payload| {
            let args: IdArgs = parse(payload, "conferencing.close_peer")?;
            let closed = m
                .close_peer(args.id)
                .map_err(|e| CommandError::handler("conferencing.close_peer", e))?;
            Ok(json!({ "closed": closed }))
        })?;

        Ok(())
    }
}

fn parse<T: for<'de> Deserialize<'de>>(
    payload: JsonValue,
    command: &str,
) -> Result<T, CommandError> {
    serde_json::from_value::<T>(payload)
        .map_err(|e| CommandError::handler(command.to_string(), e.to_string()))
}

fn session_description_to_json(desc: &RTCSessionDescription) -> JsonValue {
    json!({
        "type": sdp_type_label(desc.sdp_type),
        "sdp": desc.sdp,
    })
}

fn description_from_args(args: &DescriptionArgs) -> Result<RTCSessionDescription, String> {
    match args.kind.as_str() {
        "offer" => RTCSessionDescription::offer(args.sdp.clone())
            .map_err(|e| format!("invalid offer SDP: {e}")),
        "answer" => RTCSessionDescription::answer(args.sdp.clone())
            .map_err(|e| format!("invalid answer SDP: {e}")),
        "pranswer" => RTCSessionDescription::pranswer(args.sdp.clone())
            .map_err(|e| format!("invalid pranswer SDP: {e}")),
        other => Err(format!(
            "unknown sdp type `{other}` (expected offer/answer/pranswer)"
        )),
    }
}

fn sdp_type_label(t: RTCSdpType) -> &'static str {
    match t {
        RTCSdpType::Offer => "offer",
        RTCSdpType::Pranswer => "pranswer",
        RTCSdpType::Answer => "answer",
        RTCSdpType::Rollback => "rollback",
        RTCSdpType::Unspecified => "unspecified",
    }
}

#[derive(Debug, Deserialize)]
struct CreatePeerArgs {
    #[serde(default)]
    name: Option<String>,
    #[serde(default, rename = "ice_servers")]
    ice_servers: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
struct CreateDataChannelArgs {
    id: u64,
    label: String,
    #[serde(default)]
    ordered: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct DescriptionArgs {
    id: u64,
    sdp: String,
    #[serde(rename = "type")]
    kind: String,
}

#[derive(Debug, Deserialize)]
struct IceCandidateArgs {
    id: u64,
    candidate: String,
    #[serde(default, rename = "sdpMid")]
    sdp_mid: Option<String>,
    #[serde(default, rename = "sdpMLineIndex")]
    sdp_mline_index: Option<u16>,
    #[serde(default, rename = "usernameFragment")]
    username_fragment: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SendDataArgs {
    id: u64,
    label: String,
    #[serde(default)]
    message: Option<String>,
    #[serde(default)]
    binary: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RecvDataArgs {
    id: u64,
    #[serde(default)]
    max: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct IdArgs {
    id: u64,
}

// ── Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builder::DaemonBuilder;
    use std::time::Duration;

    fn kernel() -> crate::DaemonKernel {
        DaemonBuilder::new().with_conferencing().build().unwrap()
    }

    #[test]
    fn conferencing_module_registers_every_command() {
        let kernel = kernel();
        let caps = kernel.capabilities();
        for name in [
            "conferencing.create_peer",
            "conferencing.create_data_channel",
            "conferencing.create_offer",
            "conferencing.create_answer",
            "conferencing.set_local_description",
            "conferencing.set_remote_description",
            "conferencing.local_description",
            "conferencing.add_ice_candidate",
            "conferencing.send_data",
            "conferencing.recv_data",
            "conferencing.peer_state",
            "conferencing.list_peers",
            "conferencing.close_peer",
        ] {
            assert!(caps.contains(&name.to_string()), "missing {name}");
        }
    }

    #[test]
    fn create_peer_returns_increasing_ids_and_lists_them() {
        let mgr = ConferencingManager::new();
        let a = mgr.create_peer("alice".into(), Vec::new()).unwrap();
        let b = mgr.create_peer("bob".into(), Vec::new()).unwrap();
        assert_ne!(a, b);
        let list = mgr.list_peers();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].name, "alice");
        assert_eq!(list[1].name, "bob");
        mgr.close_all();
        assert!(mgr.list_peers().is_empty());
    }

    #[test]
    fn unknown_peer_id_surfaces_clear_error() {
        let mgr = ConferencingManager::new();
        let err = mgr.create_offer(999).unwrap_err();
        assert!(err.contains("unknown peer"));
    }

    #[test]
    fn offer_answer_handshake_via_kernel_invoke() {
        // Two peers in the same daemon: caller drives the offer, callee
        // drives the answer. We don't wait for ICE connectivity (loopback
        // ICE in-process is flaky), but we do verify the SDP exchange
        // pushes both peers through the right signaling states.
        let kernel = kernel();

        let caller = kernel
            .invoke("conferencing.create_peer", json!({ "name": "caller" }))
            .unwrap()["id"]
            .as_u64()
            .unwrap();
        let callee = kernel
            .invoke("conferencing.create_peer", json!({ "name": "callee" }))
            .unwrap()["id"]
            .as_u64()
            .unwrap();

        // Caller has to open at least one data channel before generating
        // a meaningful offer (otherwise SCTP isn't negotiated).
        kernel
            .invoke(
                "conferencing.create_data_channel",
                json!({ "id": caller, "label": "chat", "ordered": true }),
            )
            .unwrap();

        let offer = kernel
            .invoke("conferencing.create_offer", json!({ "id": caller }))
            .unwrap();
        assert_eq!(offer["type"], "offer");
        let offer_sdp = offer["sdp"].as_str().unwrap().to_string();
        assert!(!offer_sdp.is_empty());

        kernel
            .invoke(
                "conferencing.set_local_description",
                json!({ "id": caller, "type": "offer", "sdp": offer_sdp.clone() }),
            )
            .unwrap();
        kernel
            .invoke(
                "conferencing.set_remote_description",
                json!({ "id": callee, "type": "offer", "sdp": offer_sdp }),
            )
            .unwrap();

        let answer = kernel
            .invoke("conferencing.create_answer", json!({ "id": callee }))
            .unwrap();
        assert_eq!(answer["type"], "answer");
        let answer_sdp = answer["sdp"].as_str().unwrap().to_string();
        assert!(!answer_sdp.is_empty());

        kernel
            .invoke(
                "conferencing.set_local_description",
                json!({ "id": callee, "type": "answer", "sdp": answer_sdp.clone() }),
            )
            .unwrap();
        kernel
            .invoke(
                "conferencing.set_remote_description",
                json!({ "id": caller, "type": "answer", "sdp": answer_sdp }),
            )
            .unwrap();

        // Both peers should now have local descriptions exposed.
        let caller_local = kernel
            .invoke("conferencing.local_description", json!({ "id": caller }))
            .unwrap();
        assert_eq!(caller_local["type"], "offer");
        let callee_local = kernel
            .invoke("conferencing.local_description", json!({ "id": callee }))
            .unwrap();
        assert_eq!(callee_local["type"], "answer");

        // recv_data on a quiet inbox is empty.
        let drained = kernel
            .invoke("conferencing.recv_data", json!({ "id": caller }))
            .unwrap();
        assert_eq!(drained["messages"].as_array().unwrap().len(), 0);

        // Close both peers; close_peer is idempotent.
        kernel
            .invoke("conferencing.close_peer", json!({ "id": caller }))
            .unwrap();
        kernel
            .invoke("conferencing.close_peer", json!({ "id": callee }))
            .unwrap();
        let again = kernel
            .invoke("conferencing.close_peer", json!({ "id": caller }))
            .unwrap();
        assert_eq!(again["closed"], false);

        // Quick beat to let any in-flight tokio tasks finish before the
        // runtime is dropped — keeps the test output clean of stray logs.
        std::thread::sleep(Duration::from_millis(20));
    }

    #[test]
    fn send_data_to_unknown_channel_errors() {
        let mgr = ConferencingManager::new();
        let id = mgr.create_peer("solo".into(), Vec::new()).unwrap();
        let err = mgr
            .send_data(id, "missing", Bytes::from_static(b"hi"), true)
            .unwrap_err();
        assert!(err.contains("unknown data channel"));
        mgr.close_all();
    }

    #[test]
    fn description_from_args_rejects_unknown_kind() {
        let args = DescriptionArgs {
            id: 1,
            sdp: "v=0\r\n".into(),
            kind: "wat".into(),
        };
        let err = description_from_args(&args).unwrap_err();
        assert!(err.contains("unknown sdp type"));
    }

    #[test]
    fn preserved_injection_point_for_custom_manager() {
        let injected = Arc::new(ConferencingManager::new());
        let mut builder = DaemonBuilder::new();
        *builder.conferencing_manager_slot() = Some(injected.clone());
        let kernel = builder.with_conferencing().build().unwrap();
        assert!(Arc::ptr_eq(
            &kernel.conferencing_manager().unwrap(),
            &injected
        ));
    }
}
