//! `network::discovery::service` — active peer/relay scanning.
//!
//! `DiscoveryService` coordinates discovery across multiple vaults.
//! Hosts feed it relay directory responses and peer announcements;
//! it distributes records to the appropriate `VaultRoster` and
//! emits discovery events for the host's network layer to act on.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use serde::{Deserialize, Serialize};

use super::roster::{PeerRecord, PeerStatus, RelayHealth, RelayRecord, VaultRoster};

// ── Discovery events ───────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiscoveryEventKind {
    RelayDiscovered,
    PeerDiscovered,
    ScanComplete,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveryEvent {
    #[serde(rename = "type")]
    pub kind: DiscoveryEventKind,
    pub vault_id: Option<String>,
    pub id: Option<String>,
}

pub type DiscoveryListener = Box<dyn FnMut(&DiscoveryEvent)>;

// ── Relay directory entry ──────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DirectoryEntry {
    pub relay_id: String,
    pub did: String,
    pub url: String,
    pub modules: Vec<String>,
}

// ── Peer announcement ──────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PeerAnnouncement {
    pub did: String,
    pub display_name: Option<String>,
    pub vault_id: String,
    pub relay_id: Option<String>,
    pub timestamp: String,
}

// ── Listener bus ───────────────────────────────────────────────────

struct Listeners {
    next_id: u64,
    entries: Vec<(u64, DiscoveryListener)>,
}

impl Listeners {
    fn new() -> Self {
        Self {
            next_id: 0,
            entries: Vec::new(),
        }
    }

    fn add(&mut self, listener: DiscoveryListener) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        self.entries.push((id, listener));
        id
    }

    fn remove(&mut self, id: u64) {
        self.entries.retain(|(i, _)| *i != id);
    }

    fn notify(&mut self, event: &DiscoveryEvent) {
        for (_, listener) in &mut self.entries {
            listener(event);
        }
    }
}

pub struct DiscoverySubscription {
    inner: Rc<RefCell<Listeners>>,
    id: u64,
    active: bool,
}

impl DiscoverySubscription {
    pub fn unsubscribe(mut self) {
        self.active = false;
        self.inner.borrow_mut().remove(self.id);
    }
}

impl Drop for DiscoverySubscription {
    fn drop(&mut self) {
        if self.active {
            self.inner.borrow_mut().remove(self.id);
        }
    }
}

// ── DiscoveryService ───────────────────────────────────────────────

pub struct DiscoveryService {
    rosters: RefCell<HashMap<String, Rc<VaultRoster>>>,
    listeners: Rc<RefCell<Listeners>>,
}

impl DiscoveryService {
    pub fn new() -> Self {
        Self {
            rosters: RefCell::new(HashMap::new()),
            listeners: Rc::new(RefCell::new(Listeners::new())),
        }
    }

    pub fn register_vault(&self, roster: Rc<VaultRoster>) {
        let id = roster.vault_id().to_string();
        self.rosters.borrow_mut().insert(id, roster);
    }

    pub fn unregister_vault(&self, vault_id: &str) -> bool {
        self.rosters.borrow_mut().remove(vault_id).is_some()
    }

    pub fn get_roster(&self, vault_id: &str) -> Option<Rc<VaultRoster>> {
        self.rosters.borrow().get(vault_id).cloned()
    }

    pub fn vault_count(&self) -> usize {
        self.rosters.borrow().len()
    }

    pub fn process_directory(&self, vault_id: &str, entries: &[DirectoryEntry], now_iso: &str) {
        let rosters = self.rosters.borrow();
        let Some(roster) = rosters.get(vault_id) else {
            return;
        };
        for entry in entries {
            roster.add_relay(RelayRecord {
                relay_id: entry.relay_id.clone(),
                did: entry.did.clone(),
                url: entry.url.clone(),
                health: RelayHealth::Healthy,
                last_seen: now_iso.to_string(),
                modules: entry.modules.clone(),
            });
            self.notify(DiscoveryEvent {
                kind: DiscoveryEventKind::RelayDiscovered,
                vault_id: Some(vault_id.to_string()),
                id: Some(entry.relay_id.clone()),
            });
        }
        self.notify(DiscoveryEvent {
            kind: DiscoveryEventKind::ScanComplete,
            vault_id: Some(vault_id.to_string()),
            id: None,
        });
    }

    pub fn process_announcement(&self, announcement: &PeerAnnouncement) {
        let rosters = self.rosters.borrow();
        let Some(roster) = rosters.get(&announcement.vault_id) else {
            return;
        };
        let mut relay_ids = Vec::new();
        if let Some(rid) = &announcement.relay_id {
            relay_ids.push(rid.clone());
        }
        roster.add_peer(PeerRecord {
            did: announcement.did.clone(),
            display_name: announcement.display_name.clone(),
            status: PeerStatus::Known,
            last_seen: announcement.timestamp.clone(),
            relay_ids,
        });
        self.notify(DiscoveryEvent {
            kind: DiscoveryEventKind::PeerDiscovered,
            vault_id: Some(announcement.vault_id.clone()),
            id: Some(announcement.did.clone()),
        });
    }

    pub fn subscribe(&self, listener: DiscoveryListener) -> DiscoverySubscription {
        let id = self.listeners.borrow_mut().add(listener);
        DiscoverySubscription {
            inner: Rc::clone(&self.listeners),
            id,
            active: true,
        }
    }

    fn notify(&self, event: DiscoveryEvent) {
        self.listeners.borrow_mut().notify(&event);
    }
}

impl Default for DiscoveryService {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::network::discovery::roster::VaultRosterOptions;

    fn make_roster(vault_id: &str) -> Rc<VaultRoster> {
        Rc::new(VaultRoster::new(VaultRosterOptions {
            vault_id: vault_id.to_string(),
            ..Default::default()
        }))
    }

    fn make_service() -> DiscoveryService {
        let svc = DiscoveryService::new();
        svc.register_vault(make_roster("vault-1"));
        svc
    }

    fn events_for(
        svc: &DiscoveryService,
    ) -> (Rc<RefCell<Vec<DiscoveryEvent>>>, DiscoverySubscription) {
        let events: Rc<RefCell<Vec<DiscoveryEvent>>> = Rc::new(RefCell::new(Vec::new()));
        let sink = Rc::clone(&events);
        let sub = svc.subscribe(Box::new(move |e| {
            sink.borrow_mut().push(e.clone());
        }));
        (events, sub)
    }

    #[test]
    fn register_and_get_roster() {
        let svc = make_service();
        assert_eq!(svc.vault_count(), 1);
        assert!(svc.get_roster("vault-1").is_some());
        assert!(svc.get_roster("vault-99").is_none());
    }

    #[test]
    fn unregister_vault() {
        let svc = make_service();
        assert!(svc.unregister_vault("vault-1"));
        assert_eq!(svc.vault_count(), 0);
        assert!(!svc.unregister_vault("vault-1"));
    }

    #[test]
    fn process_directory_adds_relays() {
        let svc = make_service();
        let entries = vec![
            DirectoryEntry {
                relay_id: "relay-1".to_string(),
                did: "did:prism:r1".to_string(),
                url: "https://r1.example.com".to_string(),
                modules: vec!["sovereign-portals".to_string()],
            },
            DirectoryEntry {
                relay_id: "relay-2".to_string(),
                did: "did:prism:r2".to_string(),
                url: "https://r2.example.com".to_string(),
                modules: Vec::new(),
            },
        ];
        svc.process_directory("vault-1", &entries, "2026-04-18T00:00:00Z");
        let roster = svc.get_roster("vault-1").unwrap();
        assert_eq!(roster.relay_count(), 2);
    }

    #[test]
    fn process_directory_fires_events() {
        let svc = make_service();
        let (events, _sub) = events_for(&svc);
        let entries = vec![DirectoryEntry {
            relay_id: "relay-1".to_string(),
            did: "did:prism:r1".to_string(),
            url: "https://r1.example.com".to_string(),
            modules: Vec::new(),
        }];
        svc.process_directory("vault-1", &entries, "2026-04-18T00:00:00Z");
        let evs = events.borrow();
        assert_eq!(evs.len(), 2);
        assert_eq!(evs[0].kind, DiscoveryEventKind::RelayDiscovered);
        assert_eq!(evs[1].kind, DiscoveryEventKind::ScanComplete);
    }

    #[test]
    fn process_directory_ignores_unknown_vault() {
        let svc = make_service();
        let (events, _sub) = events_for(&svc);
        svc.process_directory("vault-99", &[], "2026-04-18T00:00:00Z");
        assert!(events.borrow().is_empty());
    }

    #[test]
    fn process_announcement_adds_peer() {
        let svc = make_service();
        svc.process_announcement(&PeerAnnouncement {
            did: "did:prism:alice".to_string(),
            display_name: Some("Alice".to_string()),
            vault_id: "vault-1".to_string(),
            relay_id: Some("relay-1".to_string()),
            timestamp: "2026-04-18T00:00:00Z".to_string(),
        });
        let roster = svc.get_roster("vault-1").unwrap();
        assert_eq!(roster.peer_count(), 1);
        let peer = roster.get_peer("did:prism:alice").unwrap();
        assert_eq!(peer.relay_ids, vec!["relay-1"]);
    }

    #[test]
    fn process_announcement_fires_event() {
        let svc = make_service();
        let (events, _sub) = events_for(&svc);
        svc.process_announcement(&PeerAnnouncement {
            did: "did:prism:alice".to_string(),
            display_name: None,
            vault_id: "vault-1".to_string(),
            relay_id: None,
            timestamp: "2026-04-18T00:00:00Z".to_string(),
        });
        let evs = events.borrow();
        assert_eq!(evs.len(), 1);
        assert_eq!(evs[0].kind, DiscoveryEventKind::PeerDiscovered);
        assert_eq!(evs[0].id.as_deref(), Some("did:prism:alice"));
    }

    #[test]
    fn process_announcement_ignores_unknown_vault() {
        let svc = make_service();
        let (events, _sub) = events_for(&svc);
        svc.process_announcement(&PeerAnnouncement {
            did: "did:prism:alice".to_string(),
            display_name: None,
            vault_id: "vault-99".to_string(),
            relay_id: None,
            timestamp: "2026-04-18T00:00:00Z".to_string(),
        });
        assert!(events.borrow().is_empty());
    }

    #[test]
    fn unsubscribe_stops_events() {
        let svc = make_service();
        let events: Rc<RefCell<Vec<DiscoveryEvent>>> = Rc::new(RefCell::new(Vec::new()));
        let sink = Rc::clone(&events);
        let sub = svc.subscribe(Box::new(move |e| {
            sink.borrow_mut().push(e.clone());
        }));
        sub.unsubscribe();
        svc.process_announcement(&PeerAnnouncement {
            did: "did:prism:alice".to_string(),
            display_name: None,
            vault_id: "vault-1".to_string(),
            relay_id: None,
            timestamp: "2026-04-18T00:00:00Z".to_string(),
        });
        assert!(events.borrow().is_empty());
    }
}
