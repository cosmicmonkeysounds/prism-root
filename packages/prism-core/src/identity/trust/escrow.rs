//! Encrypted key-recovery escrow. Port of `createEscrowManager` in
//! `trust/trust.ts`. Stores already-encrypted blobs by id and
//! enforces a single-claim lifecycle with optional expiry.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use chrono::{DateTime, SecondsFormat, Utc};
use indexmap::IndexMap;

use super::types::EscrowDeposit;

/// In-memory escrow manager.
#[derive(Debug, Default)]
pub struct EscrowManager {
    deposits: IndexMap<String, EscrowDeposit>,
}

pub fn create_escrow_manager() -> EscrowManager {
    EscrowManager::default()
}

impl EscrowManager {
    pub fn deposit(
        &mut self,
        depositor_id: &str,
        encrypted_payload: &str,
        expires_at: Option<String>,
    ) -> EscrowDeposit {
        let deposit = EscrowDeposit {
            id: uid("escrow"),
            depositor_id: depositor_id.to_string(),
            encrypted_payload: encrypted_payload.to_string(),
            deposited_at: Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true),
            expires_at,
            claimed: false,
        };
        self.deposits.insert(deposit.id.clone(), deposit.clone());
        deposit
    }

    pub fn claim(&mut self, deposit_id: &str) -> Option<EscrowDeposit> {
        let deposit = self.deposits.get_mut(deposit_id)?;
        if deposit.claimed {
            return None;
        }
        if let Some(exp) = deposit.expires_at.as_deref() {
            if let Ok(expiry) = DateTime::parse_from_rfc3339(exp) {
                if expiry.with_timezone(&Utc) < Utc::now() {
                    return None;
                }
            }
        }
        deposit.claimed = true;
        Some(deposit.clone())
    }

    pub fn list_deposits(&self, depositor_id: &str) -> Vec<EscrowDeposit> {
        self.deposits
            .values()
            .filter(|d| d.depositor_id == depositor_id)
            .cloned()
            .collect()
    }

    pub fn evict_expired(&mut self) -> usize {
        let now = Utc::now();
        let expired_ids: Vec<String> = self
            .deposits
            .iter()
            .filter_map(|(id, d)| {
                d.expires_at.as_deref().and_then(|exp| {
                    DateTime::parse_from_rfc3339(exp)
                        .ok()
                        .and_then(|e| (e.with_timezone(&Utc) < now).then(|| id.clone()))
                })
            })
            .collect();
        for id in &expired_ids {
            self.deposits.shift_remove(id);
        }
        expired_ids.len()
    }

    pub fn get(&self, deposit_id: &str) -> Option<&EscrowDeposit> {
        self.deposits.get(deposit_id)
    }

    pub fn list_all(&self) -> Vec<EscrowDeposit> {
        self.deposits.values().cloned().collect()
    }
}

fn uid(prefix: &str) -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    let seq = COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{prefix}-{now_ms:x}-{seq:x}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    fn past_iso() -> String {
        (Utc::now() - Duration::days(1)).to_rfc3339_opts(SecondsFormat::Millis, true)
    }

    fn future_iso() -> String {
        (Utc::now() + Duration::days(1)).to_rfc3339_opts(SecondsFormat::Millis, true)
    }

    #[test]
    fn deposits_and_retrieves() {
        let mut e = create_escrow_manager();
        let dep = e.deposit("alice", "encrypted-key-data", None);
        assert!(!dep.id.is_empty());
        assert_eq!(dep.depositor_id, "alice");
        assert_eq!(dep.encrypted_payload, "encrypted-key-data");
        assert!(!dep.claimed);
        assert!(dep.expires_at.is_none());
        assert!(e.get(&dep.id).is_some());
    }

    #[test]
    fn claims_a_deposit() {
        let mut e = create_escrow_manager();
        let dep = e.deposit("alice", "key-data", None);
        let claimed = e.claim(&dep.id);
        assert!(claimed.is_some());
        assert!(claimed.unwrap().claimed);
    }

    #[test]
    fn cannot_claim_twice() {
        let mut e = create_escrow_manager();
        let dep = e.deposit("alice", "key-data", None);
        e.claim(&dep.id);
        assert!(e.claim(&dep.id).is_none());
    }

    #[test]
    fn cannot_claim_nonexistent() {
        let mut e = create_escrow_manager();
        assert!(e.claim("fake-id").is_none());
    }

    #[test]
    fn cannot_claim_expired() {
        let mut e = create_escrow_manager();
        let dep = e.deposit("alice", "key-data", Some(past_iso()));
        assert!(e.claim(&dep.id).is_none());
    }

    #[test]
    fn lists_deposits_for_depositor() {
        let mut e = create_escrow_manager();
        e.deposit("alice", "key-1", None);
        e.deposit("alice", "key-2", None);
        e.deposit("bob", "key-3", None);
        assert_eq!(e.list_deposits("alice").len(), 2);
        assert_eq!(e.list_deposits("bob").len(), 1);
        assert_eq!(e.list_deposits("charlie").len(), 0);
    }

    #[test]
    fn evicts_expired_deposits() {
        let mut e = create_escrow_manager();
        e.deposit("alice", "expired", Some(past_iso()));
        e.deposit("bob", "valid", Some(future_iso()));
        e.deposit("charlie", "no-expiry", None);
        let evicted = e.evict_expired();
        assert_eq!(evicted, 1);
        assert_eq!(e.list_deposits("alice").len(), 0);
        assert_eq!(e.list_deposits("bob").len(), 1);
        assert_eq!(e.list_deposits("charlie").len(), 1);
    }

    #[test]
    fn deposit_with_expiry_can_still_claim() {
        let mut e = create_escrow_manager();
        let future = future_iso();
        let dep = e.deposit("alice", "key-data", Some(future.clone()));
        assert_eq!(dep.expires_at.as_deref(), Some(future.as_str()));
        assert!(e.claim(&dep.id).is_some());
    }

    #[test]
    fn list_all_returns_claimed_and_unclaimed() {
        let mut e = create_escrow_manager();
        let dep1 = e.deposit("alice", "key-1", None);
        e.deposit("bob", "key-2", None);
        e.claim(&dep1.id);
        let all = e.list_all();
        assert_eq!(all.len(), 2);
        assert_eq!(all.iter().filter(|d| d.claimed).count(), 1);
    }
}
