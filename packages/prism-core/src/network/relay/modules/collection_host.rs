//! Collection host — hosts CRDT collections on the relay.

use std::collections::HashMap;
use std::sync::RwLock;

use serde::{Deserialize, Serialize};

use crate::network::relay::module_system::{capabilities, RelayContext, RelayModule};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HostedCollection {
    pub id: String,
    pub snapshot: Vec<u8>,
    pub created_at: String,
    pub updated_at: String,
}

pub struct CollectionHost {
    collections: RwLock<HashMap<String, HostedCollection>>,
}

impl CollectionHost {
    pub fn new() -> Self {
        Self {
            collections: RwLock::new(HashMap::new()),
        }
    }

    pub fn create(&self, id: &str, now_iso: &str) -> bool {
        let mut store = self.collections.write().unwrap();
        if store.contains_key(id) {
            return false;
        }
        store.insert(
            id.to_string(),
            HostedCollection {
                id: id.to_string(),
                snapshot: Vec::new(),
                created_at: now_iso.to_string(),
                updated_at: now_iso.to_string(),
            },
        );
        true
    }

    pub fn get(&self, id: &str) -> Option<HostedCollection> {
        self.collections.read().unwrap().get(id).cloned()
    }

    pub fn list(&self) -> Vec<String> {
        self.collections.read().unwrap().keys().cloned().collect()
    }

    pub fn import_snapshot(&self, id: &str, snapshot: Vec<u8>, now_iso: &str) -> bool {
        let mut store = self.collections.write().unwrap();
        if let Some(col) = store.get_mut(id) {
            col.snapshot = snapshot;
            col.updated_at = now_iso.to_string();
            true
        } else {
            store.insert(
                id.to_string(),
                HostedCollection {
                    id: id.to_string(),
                    snapshot,
                    created_at: now_iso.to_string(),
                    updated_at: now_iso.to_string(),
                },
            );
            true
        }
    }

    pub fn export_snapshot(&self, id: &str) -> Option<Vec<u8>> {
        self.collections
            .read()
            .unwrap()
            .get(id)
            .map(|c| c.snapshot.clone())
    }

    pub fn remove(&self, id: &str) -> bool {
        self.collections.write().unwrap().remove(id).is_some()
    }

    pub fn restore(&self, collections: Vec<HostedCollection>) {
        let mut store = self.collections.write().unwrap();
        for c in collections {
            store.insert(c.id.clone(), c);
        }
    }
}

impl Default for CollectionHost {
    fn default() -> Self {
        Self::new()
    }
}

pub struct CollectionHostModule;

impl RelayModule for CollectionHostModule {
    fn name(&self) -> &str {
        "collection-host"
    }
    fn description(&self) -> &str {
        "CRDT collection hosting + sync protocol"
    }
    fn install(&self, ctx: &RelayContext) {
        ctx.set_capability(capabilities::COLLECTIONS, CollectionHost::new());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_and_import() {
        let host = CollectionHost::new();
        assert!(host.create("col-1", "2026-04-18T00:00:00Z"));
        assert!(!host.create("col-1", "2026-04-18T00:00:00Z"));

        host.import_snapshot("col-1", vec![1, 2, 3], "2026-04-18T01:00:00Z");
        let snap = host.export_snapshot("col-1").unwrap();
        assert_eq!(snap, vec![1, 2, 3]);
    }

    #[test]
    fn remove() {
        let host = CollectionHost::new();
        host.create("col-1", "2026-04-18T00:00:00Z");
        assert!(host.remove("col-1"));
        assert!(host.get("col-1").is_none());
    }
}
