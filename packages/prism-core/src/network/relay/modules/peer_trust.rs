//! Peer trust — reputation graph, bans, and content flagging.
//!
//! The core `PeerTrustGraph` is `!Send` (Rc<RefCell<>> listeners).
//! This module provides `RelayTrustGraph`, a thread-safe equivalent
//! for the multi-threaded relay server.

use std::collections::HashMap;
use std::sync::RwLock;

use serde::{Deserialize, Serialize};

use crate::network::relay::module_system::{capabilities, RelayContext, RelayModule};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PeerReputation {
    pub peer_id: String,
    pub score: i32,
    pub positive_interactions: u32,
    pub negative_interactions: u32,
    pub banned: bool,
    pub ban_reason: Option<String>,
    pub first_seen: String,
    pub last_seen: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlaggedContent {
    pub hash: String,
    pub category: String,
    pub reported_by: String,
    pub flagged_at: String,
}

pub struct RelayTrustGraph {
    peers: RwLock<HashMap<String, PeerReputation>>,
    flagged: RwLock<HashMap<String, FlaggedContent>>,
}

impl RelayTrustGraph {
    pub fn new() -> Self {
        Self {
            peers: RwLock::new(HashMap::new()),
            flagged: RwLock::new(HashMap::new()),
        }
    }

    pub fn get_peer(&self, peer_id: &str) -> Option<PeerReputation> {
        self.peers.read().unwrap().get(peer_id).cloned()
    }

    pub fn record_positive(&self, peer_id: &str, now_iso: &str) {
        let mut peers = self.peers.write().unwrap();
        let entry = peers
            .entry(peer_id.to_string())
            .or_insert_with(|| PeerReputation {
                peer_id: peer_id.to_string(),
                score: 0,
                positive_interactions: 0,
                negative_interactions: 0,
                banned: false,
                ban_reason: None,
                first_seen: now_iso.to_string(),
                last_seen: now_iso.to_string(),
            });
        entry.positive_interactions += 1;
        entry.score += 5;
        entry.last_seen = now_iso.to_string();
    }

    pub fn record_negative(&self, peer_id: &str, now_iso: &str) {
        let mut peers = self.peers.write().unwrap();
        let entry = peers
            .entry(peer_id.to_string())
            .or_insert_with(|| PeerReputation {
                peer_id: peer_id.to_string(),
                score: 0,
                positive_interactions: 0,
                negative_interactions: 0,
                banned: false,
                ban_reason: None,
                first_seen: now_iso.to_string(),
                last_seen: now_iso.to_string(),
            });
        entry.negative_interactions += 1;
        entry.score -= 10;
        entry.last_seen = now_iso.to_string();
    }

    pub fn ban(&self, peer_id: &str, reason: &str, now_iso: &str) {
        let mut peers = self.peers.write().unwrap();
        let entry = peers
            .entry(peer_id.to_string())
            .or_insert_with(|| PeerReputation {
                peer_id: peer_id.to_string(),
                score: 0,
                positive_interactions: 0,
                negative_interactions: 0,
                banned: false,
                ban_reason: None,
                first_seen: now_iso.to_string(),
                last_seen: now_iso.to_string(),
            });
        entry.banned = true;
        entry.ban_reason = Some(reason.to_string());
    }

    pub fn unban(&self, peer_id: &str) {
        if let Some(entry) = self.peers.write().unwrap().get_mut(peer_id) {
            entry.banned = false;
            entry.ban_reason = None;
        }
    }

    pub fn is_banned(&self, peer_id: &str) -> bool {
        self.peers
            .read()
            .unwrap()
            .get(peer_id)
            .is_some_and(|p| p.banned)
    }

    pub fn all_peers(&self) -> Vec<PeerReputation> {
        self.peers.read().unwrap().values().cloned().collect()
    }

    pub fn flag_content(&self, hash: &str, category: &str, reported_by: &str, now_iso: &str) {
        self.flagged.write().unwrap().insert(
            hash.to_string(),
            FlaggedContent {
                hash: hash.to_string(),
                category: category.to_string(),
                reported_by: reported_by.to_string(),
                flagged_at: now_iso.to_string(),
            },
        );
    }

    pub fn is_content_flagged(&self, hash: &str) -> bool {
        self.flagged.read().unwrap().contains_key(hash)
    }

    pub fn flagged_content(&self) -> Vec<FlaggedContent> {
        self.flagged.read().unwrap().values().cloned().collect()
    }

    pub fn check_hashes(&self, hashes: &[String]) -> Vec<FlaggedContent> {
        let flagged = self.flagged.read().unwrap();
        hashes
            .iter()
            .filter_map(|h| flagged.get(h).cloned())
            .collect()
    }

    pub fn import_flagged(&self, items: Vec<FlaggedContent>) {
        let mut flagged = self.flagged.write().unwrap();
        for item in items {
            flagged.insert(item.hash.clone(), item);
        }
    }

    pub fn restore(&self, peers: Vec<PeerReputation>, flagged: Vec<FlaggedContent>) {
        let mut peer_store = self.peers.write().unwrap();
        for p in peers {
            peer_store.insert(p.peer_id.clone(), p);
        }
        let mut flag_store = self.flagged.write().unwrap();
        for f in flagged {
            flag_store.insert(f.hash.clone(), f);
        }
    }
}

impl Default for RelayTrustGraph {
    fn default() -> Self {
        Self::new()
    }
}

pub struct PeerTrustModule;

impl RelayModule for PeerTrustModule {
    fn name(&self) -> &str {
        "peer-trust"
    }
    fn description(&self) -> &str {
        "Reputation graph, peer bans, content flagging"
    }
    fn install(&self, ctx: &RelayContext) {
        ctx.set_capability(capabilities::TRUST, RelayTrustGraph::new());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ban_and_unban() {
        let graph = RelayTrustGraph::new();
        graph.ban("alice", "spamming", "2026-04-18T00:00:00Z");
        assert!(graph.is_banned("alice"));
        graph.unban("alice");
        assert!(!graph.is_banned("alice"));
    }

    #[test]
    fn flag_content() {
        let graph = RelayTrustGraph::new();
        graph.flag_content("hash-1", "illegal", "bob", "2026-04-18T00:00:00Z");
        assert!(graph.is_content_flagged("hash-1"));
        assert!(!graph.is_content_flagged("hash-2"));
    }

    #[test]
    fn score_tracking() {
        let graph = RelayTrustGraph::new();
        graph.record_positive("alice", "2026-04-18T00:00:00Z");
        graph.record_positive("alice", "2026-04-18T00:00:00Z");
        graph.record_negative("alice", "2026-04-18T00:00:00Z");
        let peer = graph.get_peer("alice").unwrap();
        assert_eq!(peer.score, 0); // 5 + 5 - 10
        assert_eq!(peer.positive_interactions, 2);
        assert_eq!(peer.negative_interactions, 1);
    }
}
