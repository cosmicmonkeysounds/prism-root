//! Blind mailbox — E2EE store-and-forward for offline peers.

use std::collections::HashMap;
use std::sync::RwLock;

use serde::{Deserialize, Serialize};

use crate::network::relay::module_system::{capabilities, RelayContext, RelayModule};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MailboxEnvelope {
    pub id: String,
    pub from: String,
    pub to: String,
    pub ciphertext: Vec<u8>,
    pub submitted_at: String,
    pub proof_of_work: Option<String>,
    pub ttl_ms: u64,
}

pub struct BlindMailbox {
    queues: RwLock<HashMap<String, Vec<MailboxEnvelope>>>,
    default_ttl_ms: u64,
    max_envelope_size: usize,
}

impl BlindMailbox {
    pub fn new(default_ttl_ms: u64, max_envelope_size: usize) -> Self {
        Self {
            queues: RwLock::new(HashMap::new()),
            default_ttl_ms,
            max_envelope_size,
        }
    }

    pub fn deposit(&self, mut envelope: MailboxEnvelope) -> Result<(), &'static str> {
        if envelope.ciphertext.len() > self.max_envelope_size {
            return Err("envelope exceeds max size");
        }
        if envelope.ttl_ms == 0 {
            envelope.ttl_ms = self.default_ttl_ms;
        }
        self.queues
            .write()
            .unwrap()
            .entry(envelope.to.clone())
            .or_default()
            .push(envelope);
        Ok(())
    }

    pub fn collect(&self, recipient_did: &str) -> Vec<MailboxEnvelope> {
        self.queues
            .write()
            .unwrap()
            .remove(recipient_did)
            .unwrap_or_default()
    }

    pub fn pending_count(&self, recipient_did: &str) -> usize {
        self.queues
            .read()
            .unwrap()
            .get(recipient_did)
            .map_or(0, |q| q.len())
    }

    pub fn total_count(&self) -> usize {
        self.queues.read().unwrap().values().map(|q| q.len()).sum()
    }

    pub fn evict(&self, now_ms: u64) -> usize {
        let mut queues = self.queues.write().unwrap();
        let mut evicted = 0;
        queues.retain(|_, envelopes| {
            let before = envelopes.len();
            envelopes.retain(|e| {
                let submitted: u64 = e.submitted_at.parse().unwrap_or(0);
                submitted + e.ttl_ms > now_ms
            });
            evicted += before - envelopes.len();
            !envelopes.is_empty()
        });
        evicted
    }

    pub fn clear(&self) {
        self.queues.write().unwrap().clear();
    }
}

pub struct BlindMailboxModule;

impl RelayModule for BlindMailboxModule {
    fn name(&self) -> &str {
        "blind-mailbox"
    }
    fn description(&self) -> &str {
        "E2EE store-and-forward for offline peers"
    }
    fn install(&self, ctx: &RelayContext) {
        ctx.set_capability(
            capabilities::MAILBOX,
            BlindMailbox::new(
                ctx.config.default_ttl_ms,
                ctx.config.max_envelope_size_bytes,
            ),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_envelope(to: &str) -> MailboxEnvelope {
        MailboxEnvelope {
            id: "env-1".into(),
            from: "did:key:sender".into(),
            to: to.into(),
            ciphertext: vec![1, 2, 3],
            submitted_at: "1000".into(),
            proof_of_work: None,
            ttl_ms: 60_000,
        }
    }

    #[test]
    fn deposit_and_collect() {
        let mb = BlindMailbox::new(60_000, 1024);
        mb.deposit(make_envelope("alice")).unwrap();
        mb.deposit(make_envelope("alice")).unwrap();
        assert_eq!(mb.pending_count("alice"), 2);
        assert_eq!(mb.total_count(), 2);

        let collected = mb.collect("alice");
        assert_eq!(collected.len(), 2);
        assert_eq!(mb.pending_count("alice"), 0);
    }

    #[test]
    fn reject_oversized() {
        let mb = BlindMailbox::new(60_000, 2);
        let result = mb.deposit(make_envelope("alice"));
        assert!(result.is_err());
    }

    #[test]
    fn evict_expired() {
        let mb = BlindMailbox::new(60_000, 1024);
        mb.deposit(make_envelope("alice")).unwrap();
        let evicted = mb.evict(100_000);
        assert_eq!(evicted, 1);
        assert_eq!(mb.total_count(), 0);
    }
}
