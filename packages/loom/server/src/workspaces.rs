//! Multi-workspace registry — owner-keyed metadata. Each workspace's
//! CRDT payload lives in the `collection_host` capability keyed by
//! the same id; this map only carries the human-facing fields
//! (`name`, `owner`, `created_at`).
//!
//! In-memory only for v1. Persistent storage lands once the wire
//! protocol stabilises in Phase 3.

use std::collections::HashMap;
use std::sync::RwLock;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceMeta {
    pub id: String,
    pub name: String,
    pub owner: String,
    #[serde(rename = "createdAt")]
    pub created_at: String,
}

pub struct WorkspaceRegistry {
    inner: RwLock<HashMap<String, WorkspaceMeta>>,
}

impl WorkspaceRegistry {
    pub fn new() -> Self {
        Self {
            inner: RwLock::new(HashMap::new()),
        }
    }

    pub fn create(&self, name: &str, owner: &str, now_iso: &str) -> WorkspaceMeta {
        let id = format!("ws-{}", uuid::Uuid::new_v4());
        let meta = WorkspaceMeta {
            id: id.clone(),
            name: name.to_string(),
            owner: owner.to_string(),
            created_at: now_iso.to_string(),
        };
        self.inner.write().unwrap().insert(id, meta.clone());
        meta
    }

    pub fn get(&self, id: &str) -> Option<WorkspaceMeta> {
        self.inner.read().unwrap().get(id).cloned()
    }

    pub fn list_for_owner(&self, owner: &str) -> Vec<WorkspaceMeta> {
        self.inner
            .read()
            .unwrap()
            .values()
            .filter(|w| w.owner == owner)
            .cloned()
            .collect()
    }

    pub fn remove(&self, id: &str) -> Option<WorkspaceMeta> {
        self.inner.write().unwrap().remove(id)
    }
}

impl Default for WorkspaceRegistry {
    fn default() -> Self {
        Self::new()
    }
}
