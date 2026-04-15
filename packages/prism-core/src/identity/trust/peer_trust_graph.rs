//! Peer trust / reputation graph. Port of `createPeerTrustGraph` in
//! `trust/trust.ts`. Tracks positive / negative interactions, ban
//! state, and a "toxic content" hash set with an event bus for
//! downstream consumers.

use std::cell::RefCell;
use std::rc::Rc;

use chrono::{SecondsFormat, Utc};
use indexmap::IndexMap;

use super::types::{ContentHash, PeerReputation, TrustGraphEvent, TrustGraphOptions, TrustLevel};

/// Subscription handle — call [`Subscription::unsubscribe`] or drop
/// it to stop receiving events.
pub struct Subscription {
    inner: Rc<RefCell<Listeners>>,
    id: u64,
}

impl Subscription {
    pub fn unsubscribe(self) {
        // Drop impl handles the work; this method exists to match the
        // TS `onChange` → unsubscribe closure ergonomics.
    }
}

impl Drop for Subscription {
    fn drop(&mut self) {
        self.inner.borrow_mut().remove(self.id);
    }
}

type ListenerFn = Box<dyn FnMut(&TrustGraphEvent)>;

struct Listeners {
    next_id: u64,
    entries: Vec<(u64, ListenerFn)>,
}

impl Listeners {
    fn add(&mut self, listener: ListenerFn) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        self.entries.push((id, listener));
        id
    }

    fn remove(&mut self, id: u64) {
        self.entries.retain(|(i, _)| *i != id);
    }

    fn notify(&mut self, event: &TrustGraphEvent) {
        for (_, listener) in &mut self.entries {
            listener(event);
        }
    }
}

/// In-memory web-of-trust graph. `peers` / `flagged_hashes` are
/// IndexMaps so iteration order is stable (the TS `Map` behaves the
/// same way).
pub struct PeerTrustGraph {
    options: TrustGraphOptions,
    peers: IndexMap<String, PeerReputation>,
    flagged_hashes: IndexMap<String, ContentHash>,
    listeners: Rc<RefCell<Listeners>>,
}

impl std::fmt::Debug for PeerTrustGraph {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PeerTrustGraph")
            .field("options", &self.options)
            .field("peers", &self.peers)
            .field("flagged_hashes", &self.flagged_hashes)
            .finish_non_exhaustive()
    }
}

pub fn create_peer_trust_graph(options: TrustGraphOptions) -> PeerTrustGraph {
    PeerTrustGraph {
        options,
        peers: IndexMap::new(),
        flagged_hashes: IndexMap::new(),
        listeners: Rc::new(RefCell::new(Listeners {
            next_id: 0,
            entries: Vec::new(),
        })),
    }
}

impl PeerTrustGraph {
    pub fn get_peer(&self, peer_id: &str) -> Option<&PeerReputation> {
        self.peers.get(peer_id)
    }

    pub fn record_positive(&mut self, peer_id: &str) {
        let options = self.options;
        let added = self.ensure_peer(peer_id);
        let peer = self.peers.get_mut(peer_id).expect("just inserted");
        peer.positive_interactions += 1;
        peer.score = (peer.score + options.positive_weight).clamp(-100, 100);
        peer.last_seen_at = now_iso();
        peer.trust_level = compute_trust_level(
            peer.score,
            peer.banned,
            options.trusted_threshold,
            options.highly_trusted_threshold,
        );
        if added {
            self.notify(TrustGraphEvent::PeerAdded {
                peer_id: peer_id.to_string(),
            });
        }
        self.notify(TrustGraphEvent::PeerUpdated {
            peer_id: peer_id.to_string(),
        });
    }

    pub fn record_negative(&mut self, peer_id: &str) {
        let options = self.options;
        let added = self.ensure_peer(peer_id);
        let peer = self.peers.get_mut(peer_id).expect("just inserted");
        peer.negative_interactions += 1;
        peer.score = (peer.score + options.negative_weight).clamp(-100, 100);
        peer.last_seen_at = now_iso();
        peer.trust_level = compute_trust_level(
            peer.score,
            peer.banned,
            options.trusted_threshold,
            options.highly_trusted_threshold,
        );
        if added {
            self.notify(TrustGraphEvent::PeerAdded {
                peer_id: peer_id.to_string(),
            });
        }
        self.notify(TrustGraphEvent::PeerUpdated {
            peer_id: peer_id.to_string(),
        });
    }

    pub fn ban(&mut self, peer_id: &str, reason: &str) {
        let options = self.options;
        let added = self.ensure_peer(peer_id);
        let peer = self.peers.get_mut(peer_id).expect("just inserted");
        peer.banned = true;
        peer.ban_reason = Some(reason.to_string());
        peer.trust_level = compute_trust_level(
            peer.score,
            peer.banned,
            options.trusted_threshold,
            options.highly_trusted_threshold,
        );
        if added {
            self.notify(TrustGraphEvent::PeerAdded {
                peer_id: peer_id.to_string(),
            });
        }
        self.notify(TrustGraphEvent::PeerBanned {
            peer_id: peer_id.to_string(),
        });
    }

    pub fn unban(&mut self, peer_id: &str) {
        let options = self.options;
        let Some(peer) = self.peers.get_mut(peer_id) else {
            return;
        };
        if !peer.banned {
            return;
        }
        peer.banned = false;
        peer.ban_reason = None;
        peer.trust_level = compute_trust_level(
            peer.score,
            peer.banned,
            options.trusted_threshold,
            options.highly_trusted_threshold,
        );
        self.notify(TrustGraphEvent::PeerUnbanned {
            peer_id: peer_id.to_string(),
        });
    }

    pub fn is_banned(&self, peer_id: &str) -> bool {
        self.peers.get(peer_id).is_some_and(|p| p.banned)
    }

    pub fn get_peers_at_level(&self, level: TrustLevel) -> Vec<PeerReputation> {
        let order = trust_level_order();
        let min_idx = order.iter().position(|l| *l == level).unwrap_or(0);
        self.peers
            .values()
            .filter(|p| order.iter().position(|l| *l == p.trust_level).unwrap_or(0) >= min_idx)
            .cloned()
            .collect()
    }

    pub fn all_peers(&self) -> Vec<PeerReputation> {
        self.peers.values().cloned().collect()
    }

    pub fn flag_content(&mut self, hash: &str, category: &str, reported_by: &str) {
        self.flagged_hashes.insert(
            hash.to_string(),
            ContentHash {
                hash: hash.to_string(),
                category: category.to_string(),
                reported_by: reported_by.to_string(),
                reported_at: now_iso(),
            },
        );
        self.notify(TrustGraphEvent::ContentFlagged {
            content_hash: hash.to_string(),
        });
    }

    pub fn is_content_flagged(&self, hash: &str) -> bool {
        self.flagged_hashes.contains_key(hash)
    }

    pub fn flagged_content(&self) -> Vec<ContentHash> {
        self.flagged_hashes.values().cloned().collect()
    }

    pub fn on_change<F>(&self, listener: F) -> Subscription
    where
        F: FnMut(&TrustGraphEvent) + 'static,
    {
        let id = self.listeners.borrow_mut().add(Box::new(listener));
        Subscription {
            inner: Rc::clone(&self.listeners),
            id,
        }
    }

    pub fn dispose(&mut self) {
        self.peers.clear();
        self.flagged_hashes.clear();
        self.listeners.borrow_mut().entries.clear();
    }

    fn ensure_peer(&mut self, peer_id: &str) -> bool {
        if self.peers.contains_key(peer_id) {
            return false;
        }
        self.peers.insert(
            peer_id.to_string(),
            PeerReputation {
                peer_id: peer_id.to_string(),
                trust_level: TrustLevel::Unknown,
                score: 0,
                positive_interactions: 0,
                negative_interactions: 0,
                banned: false,
                ban_reason: None,
                last_seen_at: now_iso(),
            },
        );
        true
    }

    fn notify(&self, event: TrustGraphEvent) {
        self.listeners.borrow_mut().notify(&event);
    }
}

fn now_iso() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true)
}

fn compute_trust_level(
    score: i32,
    banned: bool,
    trusted_threshold: i32,
    highly_trusted_threshold: i32,
) -> TrustLevel {
    if banned {
        return TrustLevel::Untrusted;
    }
    if score >= highly_trusted_threshold {
        return TrustLevel::HighlyTrusted;
    }
    if score >= trusted_threshold {
        return TrustLevel::Trusted;
    }
    if score < 0 {
        return TrustLevel::Untrusted;
    }
    TrustLevel::Neutral
}

fn trust_level_order() -> [TrustLevel; 5] {
    [
        TrustLevel::Untrusted,
        TrustLevel::Unknown,
        TrustLevel::Neutral,
        TrustLevel::Trusted,
        TrustLevel::HighlyTrusted,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;

    #[test]
    fn starts_empty() {
        let graph = create_peer_trust_graph(TrustGraphOptions::default());
        assert!(graph.all_peers().is_empty());
        assert!(graph.get_peer("alice").is_none());
    }

    #[test]
    fn creates_peer_on_first_interaction() {
        let mut graph = create_peer_trust_graph(TrustGraphOptions::default());
        graph.record_positive("alice");
        let peer = graph.get_peer("alice").expect("peer exists");
        assert_ne!(peer.trust_level, TrustLevel::Unknown);
    }

    #[test]
    fn increases_score_on_positive() {
        let mut graph = create_peer_trust_graph(TrustGraphOptions {
            positive_weight: 10,
            ..Default::default()
        });
        graph.record_positive("alice");
        graph.record_positive("alice");
        let peer = graph.get_peer("alice").unwrap();
        assert_eq!(peer.score, 20);
        assert_eq!(peer.positive_interactions, 2);
    }

    #[test]
    fn decreases_score_on_negative() {
        let mut graph = create_peer_trust_graph(TrustGraphOptions {
            negative_weight: -15,
            ..Default::default()
        });
        graph.record_positive("alice"); // +5
        graph.record_negative("alice"); // -15 → -10
        let peer = graph.get_peer("alice").unwrap();
        assert_eq!(peer.score, -10);
        assert_eq!(peer.negative_interactions, 1);
    }

    #[test]
    fn clamps_score_to_range() {
        let mut graph = create_peer_trust_graph(TrustGraphOptions {
            positive_weight: 200,
            ..Default::default()
        });
        graph.record_positive("alice");
        assert_eq!(graph.get_peer("alice").unwrap().score, 100);
    }

    #[test]
    fn computes_trust_levels_correctly() {
        let mut graph = create_peer_trust_graph(TrustGraphOptions {
            trusted_threshold: 20,
            highly_trusted_threshold: 50,
            positive_weight: 15,
            negative_weight: -25,
        });
        graph.record_positive("alice"); // 15 → neutral
        assert_eq!(
            graph.get_peer("alice").unwrap().trust_level,
            TrustLevel::Neutral
        );
        graph.record_positive("alice"); // 30 → trusted
        assert_eq!(
            graph.get_peer("alice").unwrap().trust_level,
            TrustLevel::Trusted
        );
        graph.record_positive("alice"); // 45 → trusted
        graph.record_positive("alice"); // 60 → highly-trusted
        assert_eq!(
            graph.get_peer("alice").unwrap().trust_level,
            TrustLevel::HighlyTrusted
        );
        graph.record_negative("alice"); // 35 → trusted
        assert_eq!(
            graph.get_peer("alice").unwrap().trust_level,
            TrustLevel::Trusted
        );
    }

    #[test]
    fn bans_and_unbans_peers() {
        let mut graph = create_peer_trust_graph(TrustGraphOptions::default());
        graph.record_positive("alice");
        graph.ban("alice", "spamming");
        assert!(graph.is_banned("alice"));
        let peer = graph.get_peer("alice").unwrap();
        assert_eq!(peer.trust_level, TrustLevel::Untrusted);
        assert_eq!(peer.ban_reason.as_deref(), Some("spamming"));
        graph.unban("alice");
        assert!(!graph.is_banned("alice"));
        assert!(graph.get_peer("alice").unwrap().ban_reason.is_none());
    }

    #[test]
    fn is_banned_false_for_unknown_peer() {
        let graph = create_peer_trust_graph(TrustGraphOptions::default());
        assert!(!graph.is_banned("nobody"));
    }

    #[test]
    fn get_peers_at_level_filters_correctly() {
        let mut graph = create_peer_trust_graph(TrustGraphOptions {
            positive_weight: 50,
            ..Default::default()
        });
        graph.record_positive("alice"); // 50 → trusted
        graph.record_positive("bob"); // 50 → trusted
        graph.record_positive("bob"); // 100 → highly-trusted
        graph.record_negative("charlie"); // -10 → untrusted

        let trusted = graph.get_peers_at_level(TrustLevel::Trusted);
        assert_eq!(trusted.len(), 2);
        let highly = graph.get_peers_at_level(TrustLevel::HighlyTrusted);
        assert_eq!(highly.len(), 1);
    }

    #[test]
    fn flags_and_checks_content_hashes() {
        let mut graph = create_peer_trust_graph(TrustGraphOptions::default());
        graph.flag_content("abc123", "spam", "alice");
        assert!(graph.is_content_flagged("abc123"));
        assert!(!graph.is_content_flagged("def456"));
        assert_eq!(graph.flagged_content().len(), 1);
        assert_eq!(graph.flagged_content()[0].category, "spam");
    }

    #[test]
    fn emits_events() {
        let graph_cell = RefCell::new(create_peer_trust_graph(TrustGraphOptions::default()));
        let events: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
        let events_clone = Rc::clone(&events);
        let _sub = graph_cell.borrow().on_change(move |e| {
            let kind = match e {
                TrustGraphEvent::PeerAdded { .. } => "peer-added",
                TrustGraphEvent::PeerUpdated { .. } => "peer-updated",
                TrustGraphEvent::PeerBanned { .. } => "peer-banned",
                TrustGraphEvent::PeerUnbanned { .. } => "peer-unbanned",
                TrustGraphEvent::ContentFlagged { .. } => "content-flagged",
            };
            events_clone.borrow_mut().push(kind.to_string());
        });
        {
            let mut g = graph_cell.borrow_mut();
            g.record_positive("alice");
            g.ban("alice", "test");
            g.flag_content("hash1", "malware", "bob");
        }
        let evs = events.borrow();
        assert!(evs.iter().any(|e| e == "peer-added"));
        assert!(evs.iter().any(|e| e == "peer-updated"));
        assert!(evs.iter().any(|e| e == "peer-banned"));
        assert!(evs.iter().any(|e| e == "content-flagged"));
    }

    #[test]
    fn unsubscribe_stops_events() {
        let graph = create_peer_trust_graph(TrustGraphOptions::default());
        let events: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
        let events_clone = Rc::clone(&events);
        let sub = graph.on_change(move |_| events_clone.borrow_mut().push("e".into()));
        drop(sub);
        let mut graph = graph;
        graph.record_positive("alice");
        assert!(events.borrow().is_empty());
    }

    #[test]
    fn dispose_clears_everything() {
        let mut graph = create_peer_trust_graph(TrustGraphOptions::default());
        graph.record_positive("alice");
        graph.flag_content("hash1", "spam", "bob");
        graph.dispose();
        assert!(graph.all_peers().is_empty());
        assert!(graph.flagged_content().is_empty());
    }
}
