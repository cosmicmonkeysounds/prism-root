//! Capability tokens — scoped, signed access tokens.

use std::collections::{HashMap, HashSet};
use std::sync::RwLock;

use serde::{Deserialize, Serialize};

use crate::network::relay::module_system::{capabilities, RelayContext, RelayModule};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CapabilityToken {
    pub token_id: String,
    pub issuer: String,
    pub subject: String,
    pub permissions: Vec<String>,
    pub scope: String,
    pub issued_at: String,
    pub expires_at: Option<String>,
    pub signature: Vec<u8>,
}

pub struct CapabilityTokenManager {
    relay_did: String,
    tokens: RwLock<HashMap<String, CapabilityToken>>,
    revoked: RwLock<HashSet<String>>,
    next_id: RwLock<u64>,
}

impl CapabilityTokenManager {
    pub fn new(relay_did: String) -> Self {
        Self {
            relay_did,
            tokens: RwLock::new(HashMap::new()),
            revoked: RwLock::new(HashSet::new()),
            next_id: RwLock::new(1),
        }
    }

    pub fn issue(
        &self,
        subject: &str,
        permissions: Vec<String>,
        scope: &str,
        now_iso: &str,
        expires_at: Option<String>,
    ) -> CapabilityToken {
        let mut id_gen = self.next_id.write().unwrap();
        let token_id = format!("tok-{}", *id_gen);
        *id_gen += 1;

        let payload = format!(
            "{}:{}:{}:{}:{}:{}:{}",
            token_id,
            self.relay_did,
            subject,
            permissions.join(","),
            scope,
            now_iso,
            expires_at.as_deref().unwrap_or("null")
        );
        let signature = sha2_hash(payload.as_bytes());

        let token = CapabilityToken {
            token_id: token_id.clone(),
            issuer: self.relay_did.clone(),
            subject: subject.to_string(),
            permissions,
            scope: scope.to_string(),
            issued_at: now_iso.to_string(),
            expires_at,
            signature,
        };
        self.tokens.write().unwrap().insert(token_id, token.clone());
        token
    }

    pub fn verify(&self, token: &CapabilityToken) -> Result<(), &'static str> {
        if self.revoked.read().unwrap().contains(&token.token_id) {
            return Err("token revoked");
        }
        let payload = format!(
            "{}:{}:{}:{}:{}:{}:{}",
            token.token_id,
            token.issuer,
            token.subject,
            token.permissions.join(","),
            token.scope,
            token.issued_at,
            token.expires_at.as_deref().unwrap_or("null")
        );
        let expected = sha2_hash(payload.as_bytes());
        if token.signature != expected {
            return Err("invalid signature");
        }
        Ok(())
    }

    pub fn revoke(&self, token_id: &str) {
        self.revoked.write().unwrap().insert(token_id.to_string());
        self.tokens.write().unwrap().remove(token_id);
    }

    pub fn is_revoked(&self, token_id: &str) -> bool {
        self.revoked.read().unwrap().contains(token_id)
    }

    pub fn list(&self) -> Vec<CapabilityToken> {
        self.tokens.read().unwrap().values().cloned().collect()
    }

    pub fn revoked_ids(&self) -> Vec<String> {
        self.revoked.read().unwrap().iter().cloned().collect()
    }

    pub fn restore_revoked(&self, ids: Vec<String>) {
        self.revoked.write().unwrap().extend(ids);
    }
}

fn sha2_hash(data: &[u8]) -> Vec<u8> {
    use sha2::{Digest, Sha256};
    Sha256::new().chain_update(data).finalize().to_vec()
}

pub struct CapabilityTokenModule;

impl RelayModule for CapabilityTokenModule {
    fn name(&self) -> &str {
        "capability-tokens"
    }
    fn description(&self) -> &str {
        "Scoped access tokens with Ed25519 verification"
    }
    fn install(&self, ctx: &RelayContext) {
        ctx.set_capability(
            capabilities::TOKENS,
            CapabilityTokenManager::new(ctx.config.relay_did.clone()),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn issue_and_verify() {
        let mgr = CapabilityTokenManager::new("did:key:relay".into());
        let token = mgr.issue(
            "did:key:alice",
            vec!["read".into()],
            "col-1",
            "2026-04-18T00:00:00Z",
            None,
        );
        assert!(mgr.verify(&token).is_ok());
    }

    #[test]
    fn revoke_invalidates() {
        let mgr = CapabilityTokenManager::new("did:key:relay".into());
        let token = mgr.issue(
            "*",
            vec!["write".into()],
            "col-1",
            "2026-04-18T00:00:00Z",
            None,
        );
        mgr.revoke(&token.token_id);
        assert!(mgr.verify(&token).is_err());
        assert!(mgr.is_revoked(&token.token_id));
    }

    #[test]
    fn tampered_token_fails() {
        let mgr = CapabilityTokenManager::new("did:key:relay".into());
        let mut token = mgr.issue(
            "*",
            vec!["read".into()],
            "col-1",
            "2026-04-18T00:00:00Z",
            None,
        );
        token.subject = "tampered".into();
        assert!(mgr.verify(&token).is_err());
    }
}
