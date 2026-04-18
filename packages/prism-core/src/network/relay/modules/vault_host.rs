//! Vault host — persistent vault storage.

use std::collections::HashMap;
use std::sync::RwLock;

use serde::{Deserialize, Serialize};

use crate::network::relay::module_system::{capabilities, RelayContext, RelayModule};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HostedVault {
    pub id: String,
    pub manifest: serde_json::Value,
    pub owner_did: String,
    pub is_public: bool,
    pub hosted_at: String,
    pub updated_at: String,
    pub total_bytes: usize,
}

pub struct VaultHost {
    vaults: RwLock<HashMap<String, HostedVault>>,
    snapshots: RwLock<HashMap<String, HashMap<String, Vec<u8>>>>,
    next_id: RwLock<u64>,
}

impl VaultHost {
    pub fn new() -> Self {
        Self {
            vaults: RwLock::new(HashMap::new()),
            snapshots: RwLock::new(HashMap::new()),
            next_id: RwLock::new(1),
        }
    }

    pub fn publish(
        &self,
        manifest: serde_json::Value,
        owner_did: &str,
        is_public: bool,
        collections: HashMap<String, Vec<u8>>,
        now_iso: &str,
    ) -> HostedVault {
        let mut id_gen = self.next_id.write().unwrap();
        let id = format!("vault-{}", *id_gen);
        *id_gen += 1;

        let total_bytes: usize = collections.values().map(|v| v.len()).sum();
        let vault = HostedVault {
            id: id.clone(),
            manifest,
            owner_did: owner_did.to_string(),
            is_public,
            hosted_at: now_iso.to_string(),
            updated_at: now_iso.to_string(),
            total_bytes,
        };
        self.vaults
            .write()
            .unwrap()
            .insert(id.clone(), vault.clone());
        self.snapshots.write().unwrap().insert(id, collections);
        vault
    }

    pub fn get(&self, vault_id: &str) -> Option<HostedVault> {
        self.vaults.read().unwrap().get(vault_id).cloned()
    }

    pub fn list(&self, public_only: bool) -> Vec<HostedVault> {
        self.vaults
            .read()
            .unwrap()
            .values()
            .filter(|v| !public_only || v.is_public)
            .cloned()
            .collect()
    }

    pub fn get_snapshot(&self, vault_id: &str, collection_id: &str) -> Option<Vec<u8>> {
        self.snapshots
            .read()
            .unwrap()
            .get(vault_id)
            .and_then(|cols| cols.get(collection_id).cloned())
    }

    pub fn get_all_snapshots(&self, vault_id: &str) -> Option<HashMap<String, Vec<u8>>> {
        self.snapshots.read().unwrap().get(vault_id).cloned()
    }

    pub fn update_collections(
        &self,
        vault_id: &str,
        owner_did: &str,
        updates: HashMap<String, Vec<u8>>,
        now_iso: &str,
    ) -> bool {
        let mut vaults = self.vaults.write().unwrap();
        if let Some(vault) = vaults.get_mut(vault_id) {
            if vault.owner_did != owner_did {
                return false;
            }
            let mut snaps = self.snapshots.write().unwrap();
            let cols = snaps.entry(vault_id.to_string()).or_default();
            for (cid, data) in updates {
                cols.insert(cid, data);
            }
            vault.total_bytes = cols.values().map(|v| v.len()).sum();
            vault.updated_at = now_iso.to_string();
            true
        } else {
            false
        }
    }

    pub fn remove(&self, vault_id: &str, owner_did: &str) -> bool {
        let mut vaults = self.vaults.write().unwrap();
        if let Some(vault) = vaults.get(vault_id) {
            if vault.owner_did != owner_did {
                return false;
            }
            vaults.remove(vault_id);
            self.snapshots.write().unwrap().remove(vault_id);
            true
        } else {
            false
        }
    }

    pub fn search(&self, query: &str) -> Vec<HostedVault> {
        let q = query.to_lowercase();
        self.vaults
            .read()
            .unwrap()
            .values()
            .filter(|v| {
                v.is_public
                    && v.manifest
                        .get("name")
                        .and_then(|n| n.as_str())
                        .is_some_and(|n| n.to_lowercase().contains(&q))
            })
            .cloned()
            .collect()
    }

    pub fn restore(
        &self,
        vaults: Vec<HostedVault>,
        snapshots: HashMap<String, HashMap<String, Vec<u8>>>,
    ) {
        let mut store = self.vaults.write().unwrap();
        for v in vaults {
            store.insert(v.id.clone(), v);
        }
        let mut snap_store = self.snapshots.write().unwrap();
        for (vid, cols) in snapshots {
            snap_store.insert(vid, cols);
        }
    }
}

impl Default for VaultHost {
    fn default() -> Self {
        Self::new()
    }
}

pub struct VaultHostModule;

impl RelayModule for VaultHostModule {
    fn name(&self) -> &str {
        "vault-host"
    }
    fn description(&self) -> &str {
        "Persistent vault storage for visitor access"
    }
    fn install(&self, ctx: &RelayContext) {
        ctx.set_capability(capabilities::VAULT_HOST, VaultHost::new());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn publish_and_get() {
        let host = VaultHost::new();
        let mut cols = HashMap::new();
        cols.insert("col-1".into(), vec![1, 2, 3]);
        let vault = host.publish(
            json!({"name": "Test"}),
            "alice",
            true,
            cols,
            "2026-04-18T00:00:00Z",
        );
        assert_eq!(vault.total_bytes, 3);
        assert!(host.get(&vault.id).is_some());
    }

    #[test]
    fn owner_only_delete() {
        let host = VaultHost::new();
        let vault = host.publish(
            json!({}),
            "alice",
            true,
            HashMap::new(),
            "2026-04-18T00:00:00Z",
        );
        assert!(!host.remove(&vault.id, "bob"));
        assert!(host.remove(&vault.id, "alice"));
    }
}
