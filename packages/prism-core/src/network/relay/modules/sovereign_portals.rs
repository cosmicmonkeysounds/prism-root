//! Sovereign portals — portal registry (server-side module).

use std::collections::HashMap;
use std::sync::RwLock;

use serde::{Deserialize, Serialize};

use crate::network::relay::module_system::{capabilities, RelayContext, RelayModule};

pub type PortalLevel = u8;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PortalManifest {
    pub portal_id: String,
    pub name: String,
    pub level: PortalLevel,
    pub collection_id: String,
    pub domain: Option<String>,
    pub base_path: String,
    pub is_public: bool,
    pub access_scope: Option<String>,
    pub created_at: String,
}

pub struct PortalRegistry {
    portals: RwLock<HashMap<String, PortalManifest>>,
    next_id: RwLock<u64>,
}

impl PortalRegistry {
    pub fn new() -> Self {
        Self {
            portals: RwLock::new(HashMap::new()),
            next_id: RwLock::new(1),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn register(
        &self,
        name: &str,
        level: PortalLevel,
        collection_id: &str,
        base_path: &str,
        is_public: bool,
        domain: Option<String>,
        access_scope: Option<String>,
        now_iso: &str,
    ) -> PortalManifest {
        let mut id_gen = self.next_id.write().unwrap();
        let portal_id = format!("portal-{}", *id_gen);
        *id_gen += 1;

        let manifest = PortalManifest {
            portal_id: portal_id.clone(),
            name: name.to_string(),
            level,
            collection_id: collection_id.to_string(),
            domain,
            base_path: base_path.to_string(),
            is_public,
            access_scope,
            created_at: now_iso.to_string(),
        };
        self.portals
            .write()
            .unwrap()
            .insert(portal_id, manifest.clone());
        manifest
    }

    pub fn unregister(&self, portal_id: &str) -> bool {
        self.portals.write().unwrap().remove(portal_id).is_some()
    }

    pub fn get(&self, portal_id: &str) -> Option<PortalManifest> {
        self.portals.read().unwrap().get(portal_id).cloned()
    }

    pub fn list(&self) -> Vec<PortalManifest> {
        self.portals.read().unwrap().values().cloned().collect()
    }

    pub fn list_public(&self) -> Vec<PortalManifest> {
        self.portals
            .read()
            .unwrap()
            .values()
            .filter(|p| p.is_public)
            .cloned()
            .collect()
    }

    pub fn resolve(&self, domain: &str, path: &str) -> Option<PortalManifest> {
        self.portals
            .read()
            .unwrap()
            .values()
            .find(|p| p.domain.as_deref() == Some(domain) && path.starts_with(&p.base_path))
            .cloned()
    }

    pub fn restore(&self, portals: Vec<PortalManifest>) {
        let mut store = self.portals.write().unwrap();
        for p in portals {
            store.insert(p.portal_id.clone(), p);
        }
    }
}

impl Default for PortalRegistry {
    fn default() -> Self {
        Self::new()
    }
}

pub struct SovereignPortalModule;

impl RelayModule for SovereignPortalModule {
    fn name(&self) -> &str {
        "sovereign-portals"
    }
    fn description(&self) -> &str {
        "HTML rendering (Levels 1-4) with SEO"
    }
    fn install(&self, ctx: &RelayContext) {
        ctx.set_capability(capabilities::PORTALS, PortalRegistry::new());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn register_and_list() {
        let reg = PortalRegistry::new();
        reg.register(
            "My Portal",
            1,
            "col-1",
            "/",
            true,
            None,
            None,
            "2026-04-18T00:00:00Z",
        );
        reg.register(
            "Draft",
            1,
            "col-2",
            "/draft",
            false,
            None,
            None,
            "2026-04-18T00:00:00Z",
        );
        assert_eq!(reg.list().len(), 2);
        assert_eq!(reg.list_public().len(), 1);
    }

    #[test]
    fn unregister() {
        let reg = PortalRegistry::new();
        let p = reg.register(
            "Test",
            1,
            "col-1",
            "/",
            true,
            None,
            None,
            "2026-04-18T00:00:00Z",
        );
        assert!(reg.unregister(&p.portal_id));
        assert!(reg.get(&p.portal_id).is_none());
    }
}
