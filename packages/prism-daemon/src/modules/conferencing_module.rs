//! Conferencing module — WebRTC peer connections, data channels, audio/video
//! tracks, and room management owned by the daemon, exposed through
//! `conferencing.*` commands.
//!
//! ## Data-channel commands
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
//! ## Audio/video track commands
//!
//! | Command                                | Payload                                                        | Result                                  |
//! |----------------------------------------|----------------------------------------------------------------|-----------------------------------------|
//! | `conferencing.add_track`               | `{ id, kind: "audio"\|"video", track_id?, stream_id? }`       | `{ track_id }`                          |
//! | `conferencing.write_sample`            | `{ id, track_id, data (hex), duration_ms }`                    | `{}`                                    |
//! | `conferencing.recv_track_data`         | `{ id, max? }`                                                 | `{ samples: [{track_id, kind, data}] }` |
//! | `conferencing.list_tracks`             | `{ id }`                                                       | `{ local: [{track_id, kind}] }`         |
//! | `conferencing.remove_track`            | `{ id, track_id }`                                             | `{ removed: bool }`                     |
//!
//! ## Room commands (P2P mesh / Relay SFU)
//!
//! | Command                                | Payload                                                        | Result                                  |
//! |----------------------------------------|----------------------------------------------------------------|-----------------------------------------|
//! | `conferencing.create_room`             | `{ name? }`                                                    | `{ room_id }`                           |
//! | `conferencing.join_room`               | `{ room_id, peer_id }`                                         | `{}`                                    |
//! | `conferencing.leave_room`              | `{ room_id, peer_id }`                                         | `{ removed: bool }`                     |
//! | `conferencing.room_info`               | `{ room_id }`                                                  | `{ id, name, members: [peer_id] }`      |
//! | `conferencing.list_rooms`              | `{}`                                                           | `{ rooms: [...] }`                      |
//! | `conferencing.broadcast_data`          | `{ room_id, label, message?, binary? }`                        | `{ sent_to: N }`                        |
//!
//! ## Architecture
//!
//! WebRTC is the SPEC's chosen A/V transport (P2P data + audio + video, with
//! an SFU fallback at the Relay layer). Putting it inside the daemon means
//! every Prism surface — desktop, mobile FFI, browser WASM (eventually
//! via the C ABI), CLI — can join calls through one shared command surface
//! without each shell having to embed its own WebRTC stack.
//!
//! The [`webrtc`](https://crates.io/crates/webrtc) crate is fully async and
//! tokio-driven. The daemon kernel itself is sync — every other module
//! operates inside the synchronous `kernel.invoke` boundary — so the
//! [`ConferencingManager`] owns its own multi-threaded tokio runtime and
//! every command handler does a single `runtime.block_on` to dispatch the
//! corresponding async work. This keeps the existing transport layers
//! untouched: an IPC call, a CLI stdio frame, a UniFFI bridge, all hit
//! the same blocking entry point and the runtime is invisible to them.
//!
//! **Tracks** transport encoded media frames. The daemon does not
//! encode/decode — it passes pre-encoded Opus (audio) or VP8 (video) frames
//! between the host shell and the WebRTC transport. The host captures mic
//! audio, encodes to Opus, and pushes via `write_sample`; remote Opus frames
//! arrive via `on_track` and are drained by `recv_track_data`. The host
//! decodes and plays back. For Whisper self-transcription the host forks raw
//! PCM to `whisper.push_audio` in parallel with the Opus encode path.
//!
//! **Rooms** group peers for multi-party calls. For small groups (2–4) a
//! full-mesh P2P topology works — each daemon manages its own peer
//! connections and the room is a logical grouping. For larger groups the
//! Relay acts as an SFU, but the command surface is identical: the host
//! joins a room and broadcasts data/media to members.

use crate::builder::DaemonBuilder;
use crate::module::DaemonModule;
use crate::registry::CommandError;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value as JsonValue};
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use bytes::Bytes;
use tokio::runtime::Runtime;
use webrtc::api::media_engine::{MediaEngine, MIME_TYPE_OPUS, MIME_TYPE_VP8};
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
use webrtc::rtp_transceiver::rtp_codec::{RTCRtpCodecCapability, RTPCodecType};
use webrtc::rtp_transceiver::rtp_sender::RTCRtpSender;
use webrtc::track::track_local::track_local_static_sample::TrackLocalStaticSample;
use webrtc::track::track_local::TrackLocal;
use webrtc::track::track_remote::TrackRemote;
use webrtc_media::Sample;

// ── Manager ───────────────────────────────────────────────────────────

/// Manager for every WebRTC peer connection and room the daemon owns.
pub struct ConferencingManager {
    runtime: Arc<Runtime>,
    next_id: AtomicU64,
    peers: Mutex<HashMap<u64, Arc<PeerEntry>>>,
    next_room_id: AtomicU64,
    rooms: Mutex<HashMap<u64, RoomEntry>>,
}

/// All the per-peer state we need to drive the command surface from a sync
/// caller.
struct PeerEntry {
    name: String,
    pc: Arc<RTCPeerConnection>,
    inbox: Arc<DataInbox>,
    channels: Arc<Mutex<HashMap<String, Arc<RTCDataChannel>>>>,
    local_tracks: Arc<Mutex<HashMap<String, LocalTrackEntry>>>,
    senders: Arc<Mutex<HashMap<String, Arc<RTCRtpSender>>>>,
    track_inbox: Arc<TrackInbox>,
}

/// Inbound mailbox for data-channel messages.
#[derive(Default)]
struct DataInbox {
    queue: Mutex<VecDeque<InboundDataMessage>>,
}

/// One inbound data-channel frame.
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

// ── Track types ───────────────────────────────────────────────────────

/// Inbound media sample from a remote audio or video track, buffered by
/// the `on_track` reader task and drained by `conferencing.recv_track_data`.
#[derive(Debug, Clone, Serialize)]
pub struct InboundTrackSample {
    pub track_id: String,
    pub kind: String,
    pub data: Vec<u8>,
}

/// Inbox for inbound track media samples, mirroring the data-channel inbox
/// pattern.
#[derive(Default)]
struct TrackInbox {
    queue: Mutex<VecDeque<InboundTrackSample>>,
}

impl TrackInbox {
    fn push(&self, sample: InboundTrackSample) {
        if let Ok(mut q) = self.queue.lock() {
            q.push_back(sample);
        }
    }

    fn drain(&self, max: usize) -> Vec<InboundTrackSample> {
        let mut q = match self.queue.lock() {
            Ok(q) => q,
            Err(_) => return Vec::new(),
        };
        let n = q.len().min(max);
        q.drain(..n).collect()
    }
}

/// Entry tracking a locally-created outbound media track.
struct LocalTrackEntry {
    track: Arc<TrackLocalStaticSample>,
    kind: String,
}

// ── Room types ────────────────────────────────────────────────────────

/// A room groups peer connections for multi-party calls. Daemons can
/// manage rooms P2P for small groups (full-mesh) or delegate to the
/// Relay for larger groups (SFU topology).
struct RoomEntry {
    name: String,
    members: HashSet<u64>,
}

// ── Manager impl ──────────────────────────────────────────────────────

impl Default for ConferencingManager {
    fn default() -> Self {
        Self::new()
    }
}

impl ConferencingManager {
    /// Allocate a fresh manager backed by a multi-threaded tokio runtime.
    pub fn new() -> Self {
        let runtime = Runtime::new().expect("conferencing: failed to spawn tokio runtime");
        Self {
            runtime: Arc::new(runtime),
            next_id: AtomicU64::new(1),
            peers: Mutex::new(HashMap::new()),
            next_room_id: AtomicU64::new(1),
            rooms: Mutex::new(HashMap::new()),
        }
    }

    // ── Peer lifecycle ──────────────────────────────────────────────

    /// Allocate a new peer connection.
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
        let local_tracks: Arc<Mutex<HashMap<String, LocalTrackEntry>>> =
            Arc::new(Mutex::new(HashMap::new()));
        let senders: Arc<Mutex<HashMap<String, Arc<RTCRtpSender>>>> =
            Arc::new(Mutex::new(HashMap::new()));
        let track_inbox = Arc::new(TrackInbox::default());

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

        // When the remote adds an audio or video track, spawn a reader
        // task that buffers inbound RTP payloads into the track inbox.
        let track_inbox_for_on_track = track_inbox.clone();
        pc.on_track(Box::new(
            move |track: Arc<TrackRemote>, _receiver, _transceiver| {
                let inbox = track_inbox_for_on_track.clone();
                Box::pin(async move {
                    let track_id = track.id();
                    let kind = match track.kind() {
                        RTPCodecType::Audio => "audio",
                        RTPCodecType::Video => "video",
                        _ => "unknown",
                    }
                    .to_string();
                    // Continuously read RTP payloads and buffer them.
                    tokio::spawn(async move {
                        while let Ok((pkt, _)) = track.read_rtp().await {
                            inbox.push(InboundTrackSample {
                                track_id: track_id.clone(),
                                kind: kind.clone(),
                                data: pkt.payload.to_vec(),
                            });
                        }
                    });
                })
            },
        ));

        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let entry = Arc::new(PeerEntry {
            name,
            pc,
            inbox,
            channels,
            local_tracks,
            senders,
            track_inbox,
        });
        self.peers
            .lock()
            .map_err(|_| "peers map poisoned".to_string())?
            .insert(id, entry);
        Ok(id)
    }

    // ── Data channels ───────────────────────────────────────────────

    /// Create a new data channel on the local peer.
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

    // ── SDP / ICE ───────────────────────────────────────────────────

    pub fn create_offer(&self, id: u64) -> Result<RTCSessionDescription, String> {
        let entry = self.entry(id)?;
        let pc = entry.pc.clone();
        self.runtime
            .block_on(async move { pc.create_offer(None).await })
            .map_err(|e| format!("create_offer: {e}"))
    }

    pub fn create_answer(&self, id: u64) -> Result<RTCSessionDescription, String> {
        let entry = self.entry(id)?;
        let pc = entry.pc.clone();
        self.runtime
            .block_on(async move { pc.create_answer(None).await })
            .map_err(|e| format!("create_answer: {e}"))
    }

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

    pub fn local_description(&self, id: u64) -> Result<Option<RTCSessionDescription>, String> {
        let entry = self.entry(id)?;
        let pc = entry.pc.clone();
        Ok(self
            .runtime
            .block_on(async move { pc.local_description().await }))
    }

    pub fn add_ice_candidate(&self, id: u64, candidate: RTCIceCandidateInit) -> Result<(), String> {
        let entry = self.entry(id)?;
        let pc = entry.pc.clone();
        self.runtime
            .block_on(async move { pc.add_ice_candidate(candidate).await })
            .map_err(|e| format!("add_ice_candidate: {e}"))
    }

    // ── Data send/recv ──────────────────────────────────────────────

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

    pub fn recv_data(&self, id: u64, max: usize) -> Result<Vec<InboundDataMessage>, String> {
        let entry = self.entry(id)?;
        Ok(entry.inbox.drain(max))
    }

    // ── Peer state ──────────────────────────────────────────────────

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

    // ── Audio / video tracks ────────────────────────────────────────

    /// Add a local audio or video track to a peer connection. The track
    /// appears in the peer's next SDP negotiation (create a new
    /// offer/answer after adding tracks). Returns the track ID.
    ///
    /// The host encodes media (Opus for audio, VP8 for video) and pushes
    /// encoded frames via `write_sample`. The daemon transports — it does
    /// not encode or decode.
    pub fn add_track(
        &self,
        id: u64,
        kind: &str,
        track_id: Option<String>,
        stream_id: Option<String>,
    ) -> Result<String, String> {
        let entry = self.entry(id)?;
        let codec = match kind {
            "audio" => RTCRtpCodecCapability {
                mime_type: MIME_TYPE_OPUS.to_owned(),
                clock_rate: 48000,
                channels: 2,
                ..Default::default()
            },
            "video" => RTCRtpCodecCapability {
                mime_type: MIME_TYPE_VP8.to_owned(),
                clock_rate: 90000,
                ..Default::default()
            },
            other => {
                return Err(format!(
                    "unknown track kind `{other}` (expected audio/video)"
                ))
            }
        };
        let tid = track_id
            .unwrap_or_else(|| format!("{kind}-{}", self.next_id.fetch_add(1, Ordering::SeqCst)));
        let sid = stream_id.unwrap_or_else(|| "stream-0".to_string());
        let track = Arc::new(TrackLocalStaticSample::new(codec, tid.clone(), sid));
        let pc = entry.pc.clone();
        let sender = self
            .runtime
            .block_on(async {
                pc.add_track(track.clone() as Arc<dyn TrackLocal + Send + Sync>)
                    .await
            })
            .map_err(|e| format!("add_track: {e}"))?;
        entry
            .local_tracks
            .lock()
            .map_err(|_| "local_tracks poisoned".to_string())?
            .insert(
                tid.clone(),
                LocalTrackEntry {
                    track,
                    kind: kind.to_string(),
                },
            );
        entry
            .senders
            .lock()
            .map_err(|_| "senders poisoned".to_string())?
            .insert(tid.clone(), sender);
        Ok(tid)
    }

    /// Write an encoded media sample to a local track. For audio tracks
    /// this is an Opus frame; for video tracks a VP8 frame. The host
    /// encodes; the daemon transports.
    pub fn write_sample(
        &self,
        id: u64,
        track_id: &str,
        data: Bytes,
        duration_ms: u64,
    ) -> Result<(), String> {
        let entry = self.entry(id)?;
        let track = entry
            .local_tracks
            .lock()
            .map_err(|_| "local_tracks poisoned".to_string())?
            .get(track_id)
            .map(|e| e.track.clone())
            .ok_or_else(|| format!("unknown local track: {track_id}"))?;
        let sample = Sample {
            data,
            duration: Duration::from_millis(duration_ms),
            ..Default::default()
        };
        self.runtime
            .block_on(async { track.write_sample(&sample).await })
            .map_err(|e| format!("write_sample: {e}"))
    }

    /// Drain up to `max` queued inbound track samples (from remote tracks).
    pub fn recv_track_data(&self, id: u64, max: usize) -> Result<Vec<InboundTrackSample>, String> {
        let entry = self.entry(id)?;
        Ok(entry.track_inbox.drain(max))
    }

    /// List local tracks for a peer.
    pub fn list_tracks(&self, id: u64) -> Result<JsonValue, String> {
        let entry = self.entry(id)?;
        let local: Vec<JsonValue> = entry
            .local_tracks
            .lock()
            .map_err(|_| "local_tracks poisoned".to_string())?
            .iter()
            .map(|(tid, e)| json!({ "track_id": tid, "kind": e.kind }))
            .collect();
        Ok(json!({ "local": local }))
    }

    /// Remove a local track from the peer connection.
    pub fn remove_track(&self, id: u64, track_id: &str) -> Result<bool, String> {
        let entry = self.entry(id)?;
        let sender = entry
            .senders
            .lock()
            .map_err(|_| "senders poisoned".to_string())?
            .remove(track_id);
        entry
            .local_tracks
            .lock()
            .map_err(|_| "local_tracks poisoned".to_string())?
            .remove(track_id);
        if let Some(sender) = sender {
            self.runtime
                .block_on(async { sender.stop().await })
                .map_err(|e| format!("remove_track: {e}"))?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    // ── Room management ─────────────────────────────────────────────

    /// Create a room for multi-party calls.
    pub fn create_room(&self, name: String) -> Result<u64, String> {
        let room_id = self.next_room_id.fetch_add(1, Ordering::SeqCst);
        self.rooms
            .lock()
            .map_err(|_| "rooms map poisoned".to_string())?
            .insert(
                room_id,
                RoomEntry {
                    name,
                    members: HashSet::new(),
                },
            );
        Ok(room_id)
    }

    /// Add a peer to a room.
    pub fn join_room(&self, room_id: u64, peer_id: u64) -> Result<(), String> {
        // Verify peer exists.
        let _ = self.entry(peer_id)?;
        let mut rooms = self
            .rooms
            .lock()
            .map_err(|_| "rooms map poisoned".to_string())?;
        let room = rooms
            .get_mut(&room_id)
            .ok_or_else(|| format!("unknown room: {room_id}"))?;
        room.members.insert(peer_id);
        Ok(())
    }

    /// Remove a peer from a room.
    pub fn leave_room(&self, room_id: u64, peer_id: u64) -> Result<bool, String> {
        let mut rooms = self
            .rooms
            .lock()
            .map_err(|_| "rooms map poisoned".to_string())?;
        let room = rooms
            .get_mut(&room_id)
            .ok_or_else(|| format!("unknown room: {room_id}"))?;
        Ok(room.members.remove(&peer_id))
    }

    /// Snapshot of a room.
    pub fn room_info(&self, room_id: u64) -> Result<JsonValue, String> {
        let rooms = self
            .rooms
            .lock()
            .map_err(|_| "rooms map poisoned".to_string())?;
        let room = rooms
            .get(&room_id)
            .ok_or_else(|| format!("unknown room: {room_id}"))?;
        let mut members: Vec<u64> = room.members.iter().copied().collect();
        members.sort();
        Ok(json!({ "id": room_id, "name": room.name, "members": members }))
    }

    /// List all rooms.
    pub fn list_rooms(&self) -> Vec<JsonValue> {
        let rooms = match self.rooms.lock() {
            Ok(r) => r,
            Err(_) => return Vec::new(),
        };
        let mut out: Vec<(u64, JsonValue)> = rooms
            .iter()
            .map(|(id, r)| {
                let mut members: Vec<u64> = r.members.iter().copied().collect();
                members.sort();
                (*id, json!({ "id": id, "name": r.name, "members": members }))
            })
            .collect();
        out.sort_by_key(|(id, _)| *id);
        out.into_iter().map(|(_, v)| v).collect()
    }

    /// Send a data-channel message to every peer in a room.
    pub fn broadcast_data(
        &self,
        room_id: u64,
        label: &str,
        payload: Bytes,
        is_string: bool,
    ) -> Result<usize, String> {
        let members: Vec<u64> = {
            let rooms = self
                .rooms
                .lock()
                .map_err(|_| "rooms map poisoned".to_string())?;
            let room = rooms
                .get(&room_id)
                .ok_or_else(|| format!("unknown room: {room_id}"))?;
            room.members.iter().copied().collect()
        };
        let mut sent = 0;
        for peer_id in members {
            if self
                .send_data(peer_id, label, payload.clone(), is_string)
                .is_ok()
            {
                sent += 1;
            }
        }
        Ok(sent)
    }

    // ── Internal ────────────────────────────────────────────────────

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

        // ── Data-channel commands ───────────────────────────────────

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

        let m = manager.clone();
        registry.register("conferencing.close_peer", move |payload| {
            let args: IdArgs = parse(payload, "conferencing.close_peer")?;
            let closed = m
                .close_peer(args.id)
                .map_err(|e| CommandError::handler("conferencing.close_peer", e))?;
            Ok(json!({ "closed": closed }))
        })?;

        // ── Track commands ──────────────────────────────────────────

        let m = manager.clone();
        registry.register("conferencing.add_track", move |payload| {
            let args: AddTrackArgs = parse(payload, "conferencing.add_track")?;
            let track_id = m
                .add_track(args.id, &args.kind, args.track_id, args.stream_id)
                .map_err(|e| CommandError::handler("conferencing.add_track", e))?;
            Ok(json!({ "track_id": track_id }))
        })?;

        let m = manager.clone();
        registry.register("conferencing.write_sample", move |payload| {
            let args: WriteSampleArgs = parse(payload, "conferencing.write_sample")?;
            let data = hex::decode(&args.data).map_err(|e| {
                CommandError::handler(
                    "conferencing.write_sample",
                    format!("data must be hex: {e}"),
                )
            })?;
            m.write_sample(args.id, &args.track_id, Bytes::from(data), args.duration_ms)
                .map_err(|e| CommandError::handler("conferencing.write_sample", e))?;
            Ok(json!({}))
        })?;

        let m = manager.clone();
        registry.register("conferencing.recv_track_data", move |payload| {
            let args: RecvTrackDataArgs = parse(payload, "conferencing.recv_track_data")?;
            let max = args.max.unwrap_or(64);
            let samples = m
                .recv_track_data(args.id, max)
                .map_err(|e| CommandError::handler("conferencing.recv_track_data", e))?;
            let json_samples: Vec<JsonValue> = samples
                .into_iter()
                .map(|s| {
                    json!({
                        "track_id": s.track_id,
                        "kind": s.kind,
                        "data": hex::encode(&s.data),
                    })
                })
                .collect();
            Ok(json!({ "samples": json_samples }))
        })?;

        let m = manager.clone();
        registry.register("conferencing.list_tracks", move |payload| {
            let args: IdArgs = parse(payload, "conferencing.list_tracks")?;
            let tracks = m
                .list_tracks(args.id)
                .map_err(|e| CommandError::handler("conferencing.list_tracks", e))?;
            Ok(tracks)
        })?;

        let m = manager.clone();
        registry.register("conferencing.remove_track", move |payload| {
            let args: RemoveTrackArgs = parse(payload, "conferencing.remove_track")?;
            let removed = m
                .remove_track(args.id, &args.track_id)
                .map_err(|e| CommandError::handler("conferencing.remove_track", e))?;
            Ok(json!({ "removed": removed }))
        })?;

        // ── Room commands ───────────────────────────────────────────

        let m = manager.clone();
        registry.register("conferencing.create_room", move |payload| {
            let args: CreateRoomArgs = parse(payload, "conferencing.create_room")?;
            let room_id = m
                .create_room(args.name.unwrap_or_else(|| "room".to_string()))
                .map_err(|e| CommandError::handler("conferencing.create_room", e))?;
            Ok(json!({ "room_id": room_id }))
        })?;

        let m = manager.clone();
        registry.register("conferencing.join_room", move |payload| {
            let args: JoinRoomArgs = parse(payload, "conferencing.join_room")?;
            m.join_room(args.room_id, args.peer_id)
                .map_err(|e| CommandError::handler("conferencing.join_room", e))?;
            Ok(json!({}))
        })?;

        let m = manager.clone();
        registry.register("conferencing.leave_room", move |payload| {
            let args: LeaveRoomArgs = parse(payload, "conferencing.leave_room")?;
            let removed = m
                .leave_room(args.room_id, args.peer_id)
                .map_err(|e| CommandError::handler("conferencing.leave_room", e))?;
            Ok(json!({ "removed": removed }))
        })?;

        let m = manager.clone();
        registry.register("conferencing.room_info", move |payload| {
            let args: RoomIdArgs = parse(payload, "conferencing.room_info")?;
            let info = m
                .room_info(args.room_id)
                .map_err(|e| CommandError::handler("conferencing.room_info", e))?;
            Ok(info)
        })?;

        let m = manager.clone();
        registry.register("conferencing.list_rooms", move |_payload| {
            Ok(json!({ "rooms": m.list_rooms() }))
        })?;

        let m = manager;
        registry.register("conferencing.broadcast_data", move |payload| {
            let args: BroadcastDataArgs = parse(payload, "conferencing.broadcast_data")?;
            let (bytes, is_string) = match (args.message, args.binary) {
                (Some(text), None) => (Bytes::from(text.into_bytes()), true),
                (None, Some(hex_bytes)) => {
                    let raw = hex::decode(&hex_bytes).map_err(|e| {
                        CommandError::handler(
                            "conferencing.broadcast_data",
                            format!("binary must be hex: {e}"),
                        )
                    })?;
                    (Bytes::from(raw), false)
                }
                _ => {
                    return Err(CommandError::handler(
                        "conferencing.broadcast_data",
                        "exactly one of `message` (text) or `binary` (hex bytes) is required",
                    ))
                }
            };
            let sent = m
                .broadcast_data(args.room_id, &args.label, bytes, is_string)
                .map_err(|e| CommandError::handler("conferencing.broadcast_data", e))?;
            Ok(json!({ "sent_to": sent }))
        })?;

        Ok(())
    }
}

// ── Helpers ────────────────────────────────────────────────────────────

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

// ── Arg structs ────────────────────────────────────────────────────────

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

#[derive(Debug, Deserialize)]
struct AddTrackArgs {
    id: u64,
    kind: String,
    #[serde(default)]
    track_id: Option<String>,
    #[serde(default)]
    stream_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct WriteSampleArgs {
    id: u64,
    track_id: String,
    data: String,
    duration_ms: u64,
}

#[derive(Debug, Deserialize)]
struct RecvTrackDataArgs {
    id: u64,
    #[serde(default)]
    max: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct RemoveTrackArgs {
    id: u64,
    track_id: String,
}

#[derive(Debug, Deserialize)]
struct CreateRoomArgs {
    #[serde(default)]
    name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct JoinRoomArgs {
    room_id: u64,
    peer_id: u64,
}

#[derive(Debug, Deserialize)]
struct LeaveRoomArgs {
    room_id: u64,
    peer_id: u64,
}

#[derive(Debug, Deserialize)]
struct RoomIdArgs {
    room_id: u64,
}

#[derive(Debug, Deserialize)]
struct BroadcastDataArgs {
    room_id: u64,
    label: String,
    #[serde(default)]
    message: Option<String>,
    #[serde(default)]
    binary: Option<String>,
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
            // Tracks
            "conferencing.add_track",
            "conferencing.write_sample",
            "conferencing.recv_track_data",
            "conferencing.list_tracks",
            "conferencing.remove_track",
            // Rooms
            "conferencing.create_room",
            "conferencing.join_room",
            "conferencing.leave_room",
            "conferencing.room_info",
            "conferencing.list_rooms",
            "conferencing.broadcast_data",
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

        let caller_local = kernel
            .invoke("conferencing.local_description", json!({ "id": caller }))
            .unwrap();
        assert_eq!(caller_local["type"], "offer");
        let callee_local = kernel
            .invoke("conferencing.local_description", json!({ "id": callee }))
            .unwrap();
        assert_eq!(callee_local["type"], "answer");

        let drained = kernel
            .invoke("conferencing.recv_data", json!({ "id": caller }))
            .unwrap();
        assert_eq!(drained["messages"].as_array().unwrap().len(), 0);

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

    // ── Track tests ─────────────────────────────────────────────────

    #[test]
    fn add_track_returns_track_id_and_lists_it() {
        let mgr = ConferencingManager::new();
        let peer = mgr.create_peer("alice".into(), Vec::new()).unwrap();
        let tid = mgr
            .add_track(peer, "audio", Some("mic-1".into()), None)
            .unwrap();
        assert_eq!(tid, "mic-1");
        let tracks = mgr.list_tracks(peer).unwrap();
        let local = tracks["local"].as_array().unwrap();
        assert_eq!(local.len(), 1);
        assert_eq!(local[0]["track_id"], "mic-1");
        assert_eq!(local[0]["kind"], "audio");
        mgr.close_all();
    }

    #[test]
    fn add_track_rejects_unknown_kind() {
        let mgr = ConferencingManager::new();
        let peer = mgr.create_peer("alice".into(), Vec::new()).unwrap();
        let err = mgr.add_track(peer, "hologram", None, None).unwrap_err();
        assert!(err.contains("unknown track kind"));
        mgr.close_all();
    }

    #[test]
    fn remove_track_returns_false_for_missing() {
        let mgr = ConferencingManager::new();
        let peer = mgr.create_peer("alice".into(), Vec::new()).unwrap();
        assert!(!mgr.remove_track(peer, "nonexistent").unwrap());
        mgr.close_all();
    }

    #[test]
    fn add_track_via_kernel_invoke() {
        let kernel = kernel();
        let peer = kernel
            .invoke("conferencing.create_peer", json!({ "name": "bob" }))
            .unwrap()["id"]
            .as_u64()
            .unwrap();
        let result = kernel
            .invoke(
                "conferencing.add_track",
                json!({ "id": peer, "kind": "video", "track_id": "cam-0" }),
            )
            .unwrap();
        assert_eq!(result["track_id"], "cam-0");

        let tracks = kernel
            .invoke("conferencing.list_tracks", json!({ "id": peer }))
            .unwrap();
        assert_eq!(tracks["local"].as_array().unwrap().len(), 1);

        let removed = kernel
            .invoke(
                "conferencing.remove_track",
                json!({ "id": peer, "track_id": "cam-0" }),
            )
            .unwrap();
        assert!(removed["removed"].as_bool().unwrap());

        kernel
            .invoke("conferencing.close_peer", json!({ "id": peer }))
            .unwrap();
        std::thread::sleep(Duration::from_millis(20));
    }

    // ── Room tests ──────────────────────────────────────────────────

    #[test]
    fn room_lifecycle_via_manager() {
        let mgr = ConferencingManager::new();
        let a = mgr.create_peer("alice".into(), Vec::new()).unwrap();
        let b = mgr.create_peer("bob".into(), Vec::new()).unwrap();

        let room = mgr.create_room("standup".into()).unwrap();
        mgr.join_room(room, a).unwrap();
        mgr.join_room(room, b).unwrap();

        let info = mgr.room_info(room).unwrap();
        assert_eq!(info["name"], "standup");
        let members = info["members"].as_array().unwrap();
        assert_eq!(members.len(), 2);

        mgr.leave_room(room, a).unwrap();
        let info2 = mgr.room_info(room).unwrap();
        assert_eq!(info2["members"].as_array().unwrap().len(), 1);

        let rooms = mgr.list_rooms();
        assert_eq!(rooms.len(), 1);

        mgr.close_all();
    }

    #[test]
    fn room_lifecycle_via_kernel_invoke() {
        let kernel = kernel();
        let a = kernel
            .invoke("conferencing.create_peer", json!({ "name": "alice" }))
            .unwrap()["id"]
            .as_u64()
            .unwrap();
        let b = kernel
            .invoke("conferencing.create_peer", json!({ "name": "bob" }))
            .unwrap()["id"]
            .as_u64()
            .unwrap();

        let room_id = kernel
            .invoke("conferencing.create_room", json!({ "name": "demo" }))
            .unwrap()["room_id"]
            .as_u64()
            .unwrap();

        kernel
            .invoke(
                "conferencing.join_room",
                json!({ "room_id": room_id, "peer_id": a }),
            )
            .unwrap();
        kernel
            .invoke(
                "conferencing.join_room",
                json!({ "room_id": room_id, "peer_id": b }),
            )
            .unwrap();

        let info = kernel
            .invoke("conferencing.room_info", json!({ "room_id": room_id }))
            .unwrap();
        assert_eq!(info["members"].as_array().unwrap().len(), 2);

        let rooms = kernel.invoke("conferencing.list_rooms", json!({})).unwrap();
        assert_eq!(rooms["rooms"].as_array().unwrap().len(), 1);

        kernel
            .invoke(
                "conferencing.leave_room",
                json!({ "room_id": room_id, "peer_id": a }),
            )
            .unwrap();
        let info2 = kernel
            .invoke("conferencing.room_info", json!({ "room_id": room_id }))
            .unwrap();
        assert_eq!(info2["members"].as_array().unwrap().len(), 1);

        kernel
            .invoke("conferencing.close_peer", json!({ "id": a }))
            .unwrap();
        kernel
            .invoke("conferencing.close_peer", json!({ "id": b }))
            .unwrap();
        std::thread::sleep(Duration::from_millis(20));
    }

    #[test]
    fn join_room_rejects_unknown_peer() {
        let mgr = ConferencingManager::new();
        let room = mgr.create_room("test".into()).unwrap();
        let err = mgr.join_room(room, 999).unwrap_err();
        assert!(err.contains("unknown peer"));
    }

    #[test]
    fn join_room_rejects_unknown_room() {
        let mgr = ConferencingManager::new();
        let peer = mgr.create_peer("alice".into(), Vec::new()).unwrap();
        let err = mgr.join_room(999, peer).unwrap_err();
        assert!(err.contains("unknown room"));
        mgr.close_all();
    }
}
