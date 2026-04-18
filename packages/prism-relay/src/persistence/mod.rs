//! File-based state persistence — auto-save to relay-state.json.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use prism_core::network::relay::modules::{
    acme::SslCertificate,
    collection_host::HostedCollection,
    escrow::EscrowDeposit,
    federation::FederationPeer,
    password_auth::PasswordRecord,
    peer_trust::{FlaggedContent, PeerReputation},
    portal_templates::PortalTemplate,
    sovereign_portals::PortalManifest,
    vault_host::HostedVault,
    webhooks::WebhookConfig,
};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RelayState {
    #[serde(default)]
    pub portals: Vec<PortalManifest>,
    #[serde(default)]
    pub webhooks: Vec<WebhookConfig>,
    #[serde(default)]
    pub templates: Vec<PortalTemplate>,
    #[serde(default)]
    pub certificates: Vec<SslCertificate>,
    #[serde(default)]
    pub federation_peers: Vec<FederationPeer>,
    #[serde(default)]
    pub flagged_content: Vec<FlaggedContent>,
    #[serde(default)]
    pub peer_reputations: Vec<PeerReputation>,
    #[serde(default)]
    pub escrow_deposits: Vec<EscrowDeposit>,
    #[serde(default)]
    pub password_users: Vec<PasswordRecord>,
    #[serde(default)]
    pub revoked_tokens: Vec<String>,
    #[serde(default)]
    pub collections: Vec<HostedCollection>,
    #[serde(default)]
    pub vaults: Vec<HostedVault>,
}

pub struct FileStore {
    path: PathBuf,
}

impl FileStore {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn load(&self) -> RelayState {
        match std::fs::read_to_string(&self.path) {
            Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
            Err(_) => RelayState::default(),
        }
    }

    pub fn save(&self, state: &RelayState) -> std::io::Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(state)?;
        std::fs::write(&self.path, json)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_missing_returns_default() {
        let store = FileStore::new(PathBuf::from("/tmp/nonexistent-relay-state.json"));
        let state = store.load();
        assert!(state.portals.is_empty());
    }

    #[test]
    fn round_trip() {
        let mut state = RelayState::default();
        state.portals.push(PortalManifest {
            portal_id: "p-1".into(),
            name: "Test".into(),
            level: 1,
            collection_id: "col-1".into(),
            domain: None,
            base_path: "/".into(),
            is_public: true,
            access_scope: None,
            created_at: "2026-04-18T00:00:00Z".into(),
        });

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("relay-state.json");
        let store = FileStore::new(path);
        store.save(&state).unwrap();

        let loaded = store.load();
        assert_eq!(loaded.portals.len(), 1);
        assert_eq!(loaded.portals[0].name, "Test");
    }
}
