//! Federation — relay-to-relay mesh networking.

use std::collections::HashMap;
use std::sync::RwLock;

use serde::{Deserialize, Serialize};

use crate::network::relay::module_system::{capabilities, RelayContext, RelayModule};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FederationPeer {
    pub relay_did: String,
    pub url: String,
    pub announced_at: String,
    pub last_seen_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "lowercase")]
pub enum ForwardResult {
    Forwarded { target_relay: String },
    NoTransport,
    UnknownRelay { target_relay: String },
    Error { message: String },
}

pub trait ForwardTransport: Send + Sync {
    fn forward(&self, envelope_json: &str, peer_url: &str) -> ForwardResult;
}

pub struct FederationRegistry {
    peers: RwLock<HashMap<String, FederationPeer>>,
    transport: RwLock<Option<Box<dyn ForwardTransport>>>,
}

impl FederationRegistry {
    pub fn new() -> Self {
        Self {
            peers: RwLock::new(HashMap::new()),
            transport: RwLock::new(None),
        }
    }

    pub fn set_transport(&self, transport: Box<dyn ForwardTransport>) {
        *self.transport.write().unwrap() = Some(transport);
    }

    pub fn announce(&self, relay_did: &str, url: &str, now_iso: &str) {
        let mut peers = self.peers.write().unwrap();
        let entry = peers
            .entry(relay_did.to_string())
            .or_insert_with(|| FederationPeer {
                relay_did: relay_did.to_string(),
                url: url.to_string(),
                announced_at: now_iso.to_string(),
                last_seen_at: now_iso.to_string(),
            });
        entry.url = url.to_string();
        entry.last_seen_at = now_iso.to_string();
    }

    pub fn get_peers(&self) -> Vec<FederationPeer> {
        self.peers.read().unwrap().values().cloned().collect()
    }

    pub fn get_peer(&self, relay_did: &str) -> Option<FederationPeer> {
        self.peers.read().unwrap().get(relay_did).cloned()
    }

    pub fn remove_peer(&self, relay_did: &str) -> bool {
        self.peers.write().unwrap().remove(relay_did).is_some()
    }

    pub fn forward_envelope(&self, envelope_json: &str, target_relay: &str) -> ForwardResult {
        let transport = self.transport.read().unwrap();
        let Some(ref t) = *transport else {
            return ForwardResult::NoTransport;
        };
        let peers = self.peers.read().unwrap();
        let Some(peer) = peers.get(target_relay) else {
            return ForwardResult::UnknownRelay {
                target_relay: target_relay.to_string(),
            };
        };
        let url = peer.url.clone();
        drop(peers);
        t.forward(envelope_json, &url)
    }

    pub fn restore(&self, peers: Vec<FederationPeer>) {
        let mut store = self.peers.write().unwrap();
        for p in peers {
            store.insert(p.relay_did.clone(), p);
        }
    }
}

impl Default for FederationRegistry {
    fn default() -> Self {
        Self::new()
    }
}

pub struct FederationModule;

impl RelayModule for FederationModule {
    fn name(&self) -> &str {
        "federation"
    }
    fn description(&self) -> &str {
        "Peer discovery + cross-relay envelope forwarding"
    }
    fn install(&self, ctx: &RelayContext) {
        ctx.set_capability(capabilities::FEDERATION, FederationRegistry::new());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn announce_and_list() {
        let reg = FederationRegistry::new();
        reg.announce(
            "did:key:peer1",
            "https://peer1.example.com",
            "2026-04-18T00:00:00Z",
        );
        assert_eq!(reg.get_peers().len(), 1);
        assert!(reg.get_peer("did:key:peer1").is_some());
    }

    #[test]
    fn forward_without_transport() {
        let reg = FederationRegistry::new();
        reg.announce(
            "did:key:peer1",
            "https://peer1.example.com",
            "2026-04-18T00:00:00Z",
        );
        let result = reg.forward_envelope("{}", "did:key:peer1");
        assert!(matches!(result, ForwardResult::NoTransport));
    }

    #[test]
    fn forward_unknown_relay() {
        let reg = FederationRegistry::new();
        let result = reg.forward_envelope("{}", "did:key:unknown");
        assert!(matches!(result, ForwardResult::NoTransport));
    }
}
