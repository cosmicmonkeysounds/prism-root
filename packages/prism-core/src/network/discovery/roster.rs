//! `network::discovery::roster` — vault-scoped peer/relay address book.
//!
//! `VaultRoster` tracks known peers and relays for a given vault.
//! Each entry carries a DID, last-seen timestamp, and trust status.
//! The roster supports TTL-based expiry for stale entries and emits
//! events on changes.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

// ── Data types ─────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PeerStatus {
    Trusted,
    Known,
    Blocked,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PeerRecord {
    pub did: String,
    pub display_name: Option<String>,
    pub status: PeerStatus,
    pub last_seen: String,
    pub relay_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RelayHealth {
    Healthy,
    Degraded,
    Unreachable,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RelayRecord {
    pub relay_id: String,
    pub did: String,
    pub url: String,
    pub health: RelayHealth,
    pub last_seen: String,
    pub modules: Vec<String>,
}

// ── Events ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RosterEventKind {
    PeerAdded,
    PeerUpdated,
    PeerRemoved,
    RelayAdded,
    RelayUpdated,
    RelayRemoved,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RosterEvent {
    #[serde(rename = "type")]
    pub kind: RosterEventKind,
    pub id: String,
}

pub type RosterListener = Box<dyn FnMut(&RosterEvent)>;

// ── Options ────────────────────────────────────────────────────────

pub struct VaultRosterOptions {
    pub vault_id: String,
    pub peer_ttl_ms: u64,
    pub relay_ttl_ms: u64,
}

impl Default for VaultRosterOptions {
    fn default() -> Self {
        Self {
            vault_id: String::new(),
            peer_ttl_ms: 7 * 24 * 60 * 60 * 1000, // 7 days
            relay_ttl_ms: 24 * 60 * 60 * 1000,    // 1 day
        }
    }
}

// ── Listener bus ───────────────────────────────────────────────────

struct Listeners {
    next_id: u64,
    entries: Vec<(u64, RosterListener)>,
}

impl Listeners {
    fn new() -> Self {
        Self {
            next_id: 0,
            entries: Vec::new(),
        }
    }

    fn add(&mut self, listener: RosterListener) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        self.entries.push((id, listener));
        id
    }

    fn remove(&mut self, id: u64) {
        self.entries.retain(|(i, _)| *i != id);
    }

    fn notify(&mut self, event: &RosterEvent) {
        for (_, listener) in &mut self.entries {
            listener(event);
        }
    }
}

pub struct RosterSubscription {
    inner: Rc<RefCell<Listeners>>,
    id: u64,
    active: bool,
}

impl RosterSubscription {
    pub fn unsubscribe(mut self) {
        self.active = false;
        self.inner.borrow_mut().remove(self.id);
    }
}

impl Drop for RosterSubscription {
    fn drop(&mut self) {
        if self.active {
            self.inner.borrow_mut().remove(self.id);
        }
    }
}

// ── VaultRoster ────────────────────────────────────────────────────

pub struct VaultRoster {
    vault_id: String,
    peer_ttl_ms: u64,
    relay_ttl_ms: u64,
    peers: RefCell<IndexMap<String, PeerRecord>>,
    relays: RefCell<IndexMap<String, RelayRecord>>,
    listeners: Rc<RefCell<Listeners>>,
    now_ms: Cell<u64>,
}

impl VaultRoster {
    pub fn new(options: VaultRosterOptions) -> Self {
        Self {
            vault_id: options.vault_id,
            peer_ttl_ms: options.peer_ttl_ms,
            relay_ttl_ms: options.relay_ttl_ms,
            peers: RefCell::new(IndexMap::new()),
            relays: RefCell::new(IndexMap::new()),
            listeners: Rc::new(RefCell::new(Listeners::new())),
            now_ms: Cell::new(0),
        }
    }

    pub fn vault_id(&self) -> &str {
        &self.vault_id
    }

    pub fn set_now(&self, ms: u64) {
        self.now_ms.set(ms);
    }

    // ── Peers ──────────────────────────────────────────────────────

    pub fn add_peer(&self, peer: PeerRecord) {
        let did = peer.did.clone();
        let existed = self.peers.borrow().contains_key(&did);
        self.peers.borrow_mut().insert(did.clone(), peer);
        self.notify(RosterEvent {
            kind: if existed {
                RosterEventKind::PeerUpdated
            } else {
                RosterEventKind::PeerAdded
            },
            id: did,
        });
    }

    pub fn remove_peer(&self, did: &str) -> bool {
        if self.peers.borrow_mut().shift_remove(did).is_some() {
            self.notify(RosterEvent {
                kind: RosterEventKind::PeerRemoved,
                id: did.to_string(),
            });
            true
        } else {
            false
        }
    }

    pub fn get_peer(&self, did: &str) -> Option<PeerRecord> {
        self.peers.borrow().get(did).cloned()
    }

    pub fn list_peers(&self) -> Vec<PeerRecord> {
        self.peers.borrow().values().cloned().collect()
    }

    pub fn list_trusted_peers(&self) -> Vec<PeerRecord> {
        self.peers
            .borrow()
            .values()
            .filter(|p| p.status == PeerStatus::Trusted)
            .cloned()
            .collect()
    }

    pub fn peer_count(&self) -> usize {
        self.peers.borrow().len()
    }

    pub fn set_peer_status(&self, did: &str, status: PeerStatus) -> bool {
        let mut peers = self.peers.borrow_mut();
        if let Some(peer) = peers.get_mut(did) {
            peer.status = status;
            drop(peers);
            self.notify(RosterEvent {
                kind: RosterEventKind::PeerUpdated,
                id: did.to_string(),
            });
            true
        } else {
            false
        }
    }

    // ── Relays ─────────────────────────────────────────────────────

    pub fn add_relay(&self, relay: RelayRecord) {
        let id = relay.relay_id.clone();
        let existed = self.relays.borrow().contains_key(&id);
        self.relays.borrow_mut().insert(id.clone(), relay);
        self.notify(RosterEvent {
            kind: if existed {
                RosterEventKind::RelayUpdated
            } else {
                RosterEventKind::RelayAdded
            },
            id,
        });
    }

    pub fn remove_relay(&self, relay_id: &str) -> bool {
        if self.relays.borrow_mut().shift_remove(relay_id).is_some() {
            self.notify(RosterEvent {
                kind: RosterEventKind::RelayRemoved,
                id: relay_id.to_string(),
            });
            true
        } else {
            false
        }
    }

    pub fn get_relay(&self, relay_id: &str) -> Option<RelayRecord> {
        self.relays.borrow().get(relay_id).cloned()
    }

    pub fn list_relays(&self) -> Vec<RelayRecord> {
        self.relays.borrow().values().cloned().collect()
    }

    pub fn list_healthy_relays(&self) -> Vec<RelayRecord> {
        self.relays
            .borrow()
            .values()
            .filter(|r| r.health == RelayHealth::Healthy)
            .cloned()
            .collect()
    }

    pub fn relay_count(&self) -> usize {
        self.relays.borrow().len()
    }

    pub fn set_relay_health(&self, relay_id: &str, health: RelayHealth) -> bool {
        let mut relays = self.relays.borrow_mut();
        if let Some(relay) = relays.get_mut(relay_id) {
            relay.health = health;
            drop(relays);
            self.notify(RosterEvent {
                kind: RosterEventKind::RelayUpdated,
                id: relay_id.to_string(),
            });
            true
        } else {
            false
        }
    }

    // ── Sweep ──────────────────────────────────────────────────────

    pub fn sweep_peers(&self) -> Vec<String> {
        let now = self.now_ms.get();
        let mut evicted = Vec::new();
        let peers: Vec<(String, String)> = self
            .peers
            .borrow()
            .iter()
            .map(|(k, v)| (k.clone(), v.last_seen.clone()))
            .collect();
        for (did, last_seen) in peers {
            if let Some(seen_ms) = parse_iso_millis(&last_seen) {
                if now.saturating_sub(seen_ms) > self.peer_ttl_ms {
                    evicted.push(did);
                }
            }
        }
        for did in &evicted {
            self.remove_peer(did);
        }
        evicted
    }

    pub fn sweep_relays(&self) -> Vec<String> {
        let now = self.now_ms.get();
        let mut evicted = Vec::new();
        let relays: Vec<(String, String)> = self
            .relays
            .borrow()
            .iter()
            .map(|(k, v)| (k.clone(), v.last_seen.clone()))
            .collect();
        for (id, last_seen) in relays {
            if let Some(seen_ms) = parse_iso_millis(&last_seen) {
                if now.saturating_sub(seen_ms) > self.relay_ttl_ms {
                    evicted.push(id);
                }
            }
        }
        for id in &evicted {
            self.remove_relay(id);
        }
        evicted
    }

    // ── Subscriptions ──────────────────────────────────────────────

    pub fn subscribe(&self, listener: RosterListener) -> RosterSubscription {
        let id = self.listeners.borrow_mut().add(listener);
        RosterSubscription {
            inner: Rc::clone(&self.listeners),
            id,
            active: true,
        }
    }

    // ── Serialization ──────────────────────────────────────────────

    pub fn export(&self) -> RosterSnapshot {
        RosterSnapshot {
            vault_id: self.vault_id.clone(),
            peers: self.list_peers(),
            relays: self.list_relays(),
        }
    }

    pub fn import(&self, snapshot: &RosterSnapshot) {
        for peer in &snapshot.peers {
            self.add_peer(peer.clone());
        }
        for relay in &snapshot.relays {
            self.add_relay(relay.clone());
        }
    }

    // ── Internal ───────────────────────────────────────────────────

    fn notify(&self, event: RosterEvent) {
        self.listeners.borrow_mut().notify(&event);
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RosterSnapshot {
    pub vault_id: String,
    pub peers: Vec<PeerRecord>,
    pub relays: Vec<RelayRecord>,
}

fn parse_iso_millis(iso: &str) -> Option<u64> {
    let dt = chrono::DateTime::parse_from_rfc3339(iso).ok()?;
    let utc = dt.with_timezone(&chrono::Utc);
    let secs = utc.timestamp();
    if secs < 0 {
        return Some(0);
    }
    Some((secs as u64) * 1000 + u64::from(utc.timestamp_subsec_millis()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_peer(did: &str) -> PeerRecord {
        PeerRecord {
            did: did.to_string(),
            display_name: Some(format!("Peer {did}")),
            status: PeerStatus::Known,
            last_seen: "2026-04-18T00:00:00.000Z".to_string(),
            relay_ids: Vec::new(),
        }
    }

    fn make_relay(id: &str) -> RelayRecord {
        RelayRecord {
            relay_id: id.to_string(),
            did: format!("did:prism:{id}"),
            url: format!("https://{id}.example.com"),
            health: RelayHealth::Healthy,
            last_seen: "2026-04-18T00:00:00.000Z".to_string(),
            modules: vec!["sovereign-portals".to_string()],
        }
    }

    fn make_roster() -> VaultRoster {
        VaultRoster::new(VaultRosterOptions {
            vault_id: "vault-1".to_string(),
            peer_ttl_ms: 30_000,
            relay_ttl_ms: 30_000,
        })
    }

    fn events_for(roster: &VaultRoster) -> (Rc<RefCell<Vec<RosterEvent>>>, RosterSubscription) {
        let events: Rc<RefCell<Vec<RosterEvent>>> = Rc::new(RefCell::new(Vec::new()));
        let sink = Rc::clone(&events);
        let sub = roster.subscribe(Box::new(move |e| {
            sink.borrow_mut().push(e.clone());
        }));
        (events, sub)
    }

    #[test]
    fn add_and_list_peers() {
        let roster = make_roster();
        roster.add_peer(make_peer("did:prism:alice"));
        roster.add_peer(make_peer("did:prism:bob"));
        assert_eq!(roster.peer_count(), 2);
        assert_eq!(roster.list_peers().len(), 2);
    }

    #[test]
    fn get_peer_by_did() {
        let roster = make_roster();
        roster.add_peer(make_peer("did:prism:alice"));
        let peer = roster.get_peer("did:prism:alice").unwrap();
        assert_eq!(peer.did, "did:prism:alice");
        assert!(roster.get_peer("did:prism:unknown").is_none());
    }

    #[test]
    fn remove_peer() {
        let roster = make_roster();
        roster.add_peer(make_peer("did:prism:alice"));
        assert!(roster.remove_peer("did:prism:alice"));
        assert_eq!(roster.peer_count(), 0);
        assert!(!roster.remove_peer("did:prism:alice"));
    }

    #[test]
    fn set_peer_status() {
        let roster = make_roster();
        roster.add_peer(make_peer("did:prism:alice"));
        assert!(roster.set_peer_status("did:prism:alice", PeerStatus::Trusted));
        assert_eq!(
            roster.get_peer("did:prism:alice").unwrap().status,
            PeerStatus::Trusted
        );
        assert!(!roster.set_peer_status("did:prism:unknown", PeerStatus::Blocked));
    }

    #[test]
    fn list_trusted_peers() {
        let roster = make_roster();
        roster.add_peer(make_peer("did:prism:alice"));
        roster.add_peer(make_peer("did:prism:bob"));
        roster.set_peer_status("did:prism:alice", PeerStatus::Trusted);
        let trusted = roster.list_trusted_peers();
        assert_eq!(trusted.len(), 1);
        assert_eq!(trusted[0].did, "did:prism:alice");
    }

    #[test]
    fn add_and_list_relays() {
        let roster = make_roster();
        roster.add_relay(make_relay("relay-1"));
        roster.add_relay(make_relay("relay-2"));
        assert_eq!(roster.relay_count(), 2);
    }

    #[test]
    fn get_relay() {
        let roster = make_roster();
        roster.add_relay(make_relay("relay-1"));
        assert!(roster.get_relay("relay-1").is_some());
        assert!(roster.get_relay("relay-99").is_none());
    }

    #[test]
    fn remove_relay() {
        let roster = make_roster();
        roster.add_relay(make_relay("relay-1"));
        assert!(roster.remove_relay("relay-1"));
        assert_eq!(roster.relay_count(), 0);
        assert!(!roster.remove_relay("relay-1"));
    }

    #[test]
    fn set_relay_health() {
        let roster = make_roster();
        roster.add_relay(make_relay("relay-1"));
        assert!(roster.set_relay_health("relay-1", RelayHealth::Degraded));
        assert_eq!(
            roster.get_relay("relay-1").unwrap().health,
            RelayHealth::Degraded
        );
    }

    #[test]
    fn list_healthy_relays() {
        let roster = make_roster();
        roster.add_relay(make_relay("relay-1"));
        roster.add_relay(make_relay("relay-2"));
        roster.set_relay_health("relay-2", RelayHealth::Unreachable);
        let healthy = roster.list_healthy_relays();
        assert_eq!(healthy.len(), 1);
        assert_eq!(healthy[0].relay_id, "relay-1");
    }

    #[test]
    fn peer_events_fire_on_add_update_remove() {
        let roster = make_roster();
        let (events, _sub) = events_for(&roster);
        roster.add_peer(make_peer("did:prism:alice"));
        roster.add_peer(make_peer("did:prism:alice"));
        roster.remove_peer("did:prism:alice");
        let evs = events.borrow();
        assert_eq!(evs.len(), 3);
        assert_eq!(evs[0].kind, RosterEventKind::PeerAdded);
        assert_eq!(evs[1].kind, RosterEventKind::PeerUpdated);
        assert_eq!(evs[2].kind, RosterEventKind::PeerRemoved);
    }

    #[test]
    fn relay_events_fire_on_add_update_remove() {
        let roster = make_roster();
        let (events, _sub) = events_for(&roster);
        roster.add_relay(make_relay("relay-1"));
        roster.add_relay(make_relay("relay-1"));
        roster.remove_relay("relay-1");
        let evs = events.borrow();
        assert_eq!(evs.len(), 3);
        assert_eq!(evs[0].kind, RosterEventKind::RelayAdded);
        assert_eq!(evs[1].kind, RosterEventKind::RelayUpdated);
        assert_eq!(evs[2].kind, RosterEventKind::RelayRemoved);
    }

    #[test]
    fn sweep_evicts_stale_peers() {
        let roster = make_roster();
        roster.add_peer(make_peer("did:prism:alice"));
        // last_seen is 2026-04-18T00:00:00.000Z
        roster.set_now(1_776_470_400_000 + 31_000);
        let evicted = roster.sweep_peers();
        assert_eq!(evicted, vec!["did:prism:alice"]);
        assert_eq!(roster.peer_count(), 0);
    }

    #[test]
    fn sweep_keeps_fresh_peers() {
        let roster = make_roster();
        roster.add_peer(make_peer("did:prism:alice"));
        roster.set_now(1_776_470_400_000 + 10_000);
        let evicted = roster.sweep_peers();
        assert!(evicted.is_empty());
        assert_eq!(roster.peer_count(), 1);
    }

    #[test]
    fn sweep_evicts_stale_relays() {
        let roster = make_roster();
        roster.add_relay(make_relay("relay-1"));
        roster.set_now(1_776_470_400_000 + 31_000);
        let evicted = roster.sweep_relays();
        assert_eq!(evicted, vec!["relay-1"]);
        assert_eq!(roster.relay_count(), 0);
    }

    #[test]
    fn export_and_import() {
        let roster = make_roster();
        roster.add_peer(make_peer("did:prism:alice"));
        roster.add_relay(make_relay("relay-1"));
        let snapshot = roster.export();

        let roster2 = make_roster();
        roster2.import(&snapshot);
        assert_eq!(roster2.peer_count(), 1);
        assert_eq!(roster2.relay_count(), 1);
    }

    #[test]
    fn unsubscribe_stops_events() {
        let roster = make_roster();
        let events: Rc<RefCell<Vec<RosterEvent>>> = Rc::new(RefCell::new(Vec::new()));
        let sink = Rc::clone(&events);
        let sub = roster.subscribe(Box::new(move |e| {
            sink.borrow_mut().push(e.clone());
        }));
        sub.unsubscribe();
        roster.add_peer(make_peer("did:prism:alice"));
        assert!(events.borrow().is_empty());
    }

    #[test]
    fn vault_id_accessor() {
        let roster = make_roster();
        assert_eq!(roster.vault_id(), "vault-1");
    }
}
