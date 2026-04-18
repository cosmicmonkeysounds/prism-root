//! WebRTC signaling — P2P/SFU connection negotiation.

use std::collections::HashMap;
use std::sync::RwLock;

use serde::{Deserialize, Serialize};

use crate::network::relay::module_system::{capabilities, RelayContext, RelayModule};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SignalType {
    Offer,
    Answer,
    IceCandidate,
    Leave,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SignalMessage {
    #[serde(rename = "type")]
    pub signal_type: SignalType,
    pub from: String,
    pub to: String,
    pub room_id: String,
    pub payload: serde_json::Value,
    pub timestamp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SignalingPeer {
    pub peer_id: String,
    pub display_name: Option<String>,
    pub joined_at: String,
    pub metadata: Option<serde_json::Value>,
}

struct Room {
    room_id: String,
    peers: HashMap<String, SignalingPeer>,
    #[allow(clippy::type_complexity)]
    deliver_fns: HashMap<String, Box<dyn Fn(&SignalMessage) + Send + Sync>>,
    created_at: String,
    #[allow(dead_code)]
    max_peers: usize,
}

pub struct SignalingHub {
    rooms: RwLock<HashMap<String, Room>>,
}

impl SignalingHub {
    pub fn new() -> Self {
        Self {
            rooms: RwLock::new(HashMap::new()),
        }
    }

    pub fn join(
        &self,
        room_id: &str,
        peer: SignalingPeer,
        deliver: Box<dyn Fn(&SignalMessage) + Send + Sync>,
        now_iso: &str,
    ) -> Vec<SignalingPeer> {
        let mut rooms = self.rooms.write().unwrap();
        let room = rooms.entry(room_id.to_string()).or_insert_with(|| Room {
            room_id: room_id.to_string(),
            peers: HashMap::new(),
            deliver_fns: HashMap::new(),
            created_at: now_iso.to_string(),
            max_peers: 0,
        });
        let existing: Vec<SignalingPeer> = room.peers.values().cloned().collect();
        room.peers.insert(peer.peer_id.clone(), peer.clone());
        room.deliver_fns.insert(peer.peer_id.clone(), deliver);
        existing
    }

    pub fn leave(&self, room_id: &str, peer_id: &str, now_iso: &str) {
        let mut rooms = self.rooms.write().unwrap();
        if let Some(room) = rooms.get_mut(room_id) {
            room.peers.remove(peer_id);
            room.deliver_fns.remove(peer_id);
            let leave_msg = SignalMessage {
                signal_type: SignalType::Leave,
                from: peer_id.to_string(),
                to: String::new(),
                room_id: room_id.to_string(),
                payload: serde_json::Value::Null,
                timestamp: now_iso.to_string(),
            };
            for deliver in room.deliver_fns.values() {
                deliver(&leave_msg);
            }
        }
    }

    pub fn relay_signal(&self, message: &SignalMessage) -> bool {
        let rooms = self.rooms.read().unwrap();
        if let Some(room) = rooms.get(&message.room_id) {
            if let Some(deliver) = room.deliver_fns.get(&message.to) {
                deliver(message);
                return true;
            }
        }
        false
    }

    pub fn list_rooms(&self) -> Vec<(String, usize, String)> {
        self.rooms
            .read()
            .unwrap()
            .values()
            .map(|r| (r.room_id.clone(), r.peers.len(), r.created_at.clone()))
            .collect()
    }

    pub fn get_peers(&self, room_id: &str) -> Vec<SignalingPeer> {
        self.rooms
            .read()
            .unwrap()
            .get(room_id)
            .map(|r| r.peers.values().cloned().collect())
            .unwrap_or_default()
    }

    pub fn evict_empty_rooms(&self) -> usize {
        let mut rooms = self.rooms.write().unwrap();
        let before = rooms.len();
        rooms.retain(|_, r| !r.peers.is_empty());
        before - rooms.len()
    }
}

impl Default for SignalingHub {
    fn default() -> Self {
        Self::new()
    }
}

pub struct WebrtcSignalingModule;

impl RelayModule for WebrtcSignalingModule {
    fn name(&self) -> &str {
        "webrtc-signaling"
    }
    fn description(&self) -> &str {
        "P2P/SFU WebRTC connection negotiation"
    }
    fn install(&self, ctx: &RelayContext) {
        ctx.set_capability(capabilities::SIGNALING, SignalingHub::new());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;

    #[test]
    fn join_and_leave() {
        let hub = SignalingHub::new();
        let count = Arc::new(AtomicU32::new(0));
        let c = Arc::clone(&count);

        let existing = hub.join(
            "room-1",
            SignalingPeer {
                peer_id: "alice".into(),
                display_name: None,
                joined_at: "now".into(),
                metadata: None,
            },
            Box::new(move |_| {
                c.fetch_add(1, Ordering::Relaxed);
            }),
            "now",
        );
        assert!(existing.is_empty());

        let existing = hub.join(
            "room-1",
            SignalingPeer {
                peer_id: "bob".into(),
                display_name: None,
                joined_at: "now".into(),
                metadata: None,
            },
            Box::new(|_| {}),
            "now",
        );
        assert_eq!(existing.len(), 1);

        hub.leave("room-1", "bob", "now");
        assert!(count.load(Ordering::Relaxed) > 0);
    }

    #[test]
    fn evict_empty() {
        let hub = SignalingHub::new();
        hub.join(
            "room-1",
            SignalingPeer {
                peer_id: "alice".into(),
                display_name: None,
                joined_at: "now".into(),
                metadata: None,
            },
            Box::new(|_| {}),
            "now",
        );
        hub.leave("room-1", "alice", "now");
        let evicted = hub.evict_empty_rooms();
        assert_eq!(evicted, 1);
    }
}
