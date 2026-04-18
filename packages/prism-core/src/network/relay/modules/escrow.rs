//! Escrow — blind key recovery deposits.
//!
//! Thread-safe wrapper over the escrow deposit pattern for the relay server.

use std::collections::HashMap;
use std::sync::RwLock;

use serde::{Deserialize, Serialize};

use crate::network::relay::module_system::{capabilities, RelayContext, RelayModule};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EscrowDeposit {
    pub id: String,
    pub depositor_id: String,
    pub encrypted_payload: String,
    pub created_at: String,
    pub expires_at: Option<String>,
    pub claimed: bool,
}

pub struct RelayEscrowManager {
    deposits: RwLock<HashMap<String, EscrowDeposit>>,
    next_id: RwLock<u64>,
}

impl RelayEscrowManager {
    pub fn new() -> Self {
        Self {
            deposits: RwLock::new(HashMap::new()),
            next_id: RwLock::new(1),
        }
    }

    pub fn deposit(
        &self,
        depositor_id: &str,
        encrypted_payload: &str,
        expires_at: Option<String>,
        now_iso: &str,
    ) -> EscrowDeposit {
        let mut id_gen = self.next_id.write().unwrap();
        let id = format!("esc-{}", *id_gen);
        *id_gen += 1;

        let deposit = EscrowDeposit {
            id: id.clone(),
            depositor_id: depositor_id.to_string(),
            encrypted_payload: encrypted_payload.to_string(),
            created_at: now_iso.to_string(),
            expires_at,
            claimed: false,
        };
        self.deposits.write().unwrap().insert(id, deposit.clone());
        deposit
    }

    pub fn claim(&self, deposit_id: &str) -> Option<EscrowDeposit> {
        let mut deposits = self.deposits.write().unwrap();
        if let Some(deposit) = deposits.get_mut(deposit_id) {
            if deposit.claimed {
                return None;
            }
            deposit.claimed = true;
            Some(deposit.clone())
        } else {
            None
        }
    }

    pub fn list_deposits(&self, depositor_id: &str) -> Vec<EscrowDeposit> {
        self.deposits
            .read()
            .unwrap()
            .values()
            .filter(|d| d.depositor_id == depositor_id && !d.claimed)
            .cloned()
            .collect()
    }

    pub fn evict_expired(&self, now_iso: &str) -> usize {
        let mut deposits = self.deposits.write().unwrap();
        let before = deposits.len();
        deposits
            .retain(|_, d| d.expires_at.as_deref().is_none_or(|exp| exp > now_iso) && !d.claimed);
        before - deposits.len()
    }

    pub fn get(&self, deposit_id: &str) -> Option<EscrowDeposit> {
        self.deposits.read().unwrap().get(deposit_id).cloned()
    }

    pub fn restore(&self, deposits: Vec<EscrowDeposit>) {
        let mut store = self.deposits.write().unwrap();
        for d in deposits {
            store.insert(d.id.clone(), d);
        }
    }
}

impl Default for RelayEscrowManager {
    fn default() -> Self {
        Self::new()
    }
}

pub struct EscrowModule;

impl RelayModule for EscrowModule {
    fn name(&self) -> &str {
        "escrow"
    }
    fn description(&self) -> &str {
        "Blind escrow key recovery deposits"
    }
    fn install(&self, ctx: &RelayContext) {
        ctx.set_capability(capabilities::ESCROW, RelayEscrowManager::new());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deposit_and_claim() {
        let mgr = RelayEscrowManager::new();
        let dep = mgr.deposit("alice", "encrypted-data", None, "2026-04-18T00:00:00Z");
        assert!(mgr.get(&dep.id).is_some());

        let claimed = mgr.claim(&dep.id).unwrap();
        assert!(claimed.claimed);

        // Double claim fails
        assert!(mgr.claim(&dep.id).is_none());
    }

    #[test]
    fn list_by_depositor() {
        let mgr = RelayEscrowManager::new();
        mgr.deposit("alice", "data-1", None, "2026-04-18T00:00:00Z");
        mgr.deposit("bob", "data-2", None, "2026-04-18T00:00:00Z");
        mgr.deposit("alice", "data-3", None, "2026-04-18T00:00:00Z");
        assert_eq!(mgr.list_deposits("alice").len(), 2);
    }
}
