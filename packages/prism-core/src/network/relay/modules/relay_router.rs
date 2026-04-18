//! Relay router — zero-knowledge envelope routing.

use std::collections::HashMap;
use std::sync::RwLock;

use serde::{Deserialize, Serialize};

use super::blind_mailbox::{BlindMailbox, MailboxEnvelope};
use crate::network::relay::module_system::{capabilities, RelayContext, RelayModule};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "lowercase")]
pub enum RouteResult {
    Delivered {
        recipient_did: String,
    },
    Queued {
        recipient_did: String,
        mailbox_size: usize,
    },
    Rejected {
        reason: String,
    },
}

type DeliverFn = Box<dyn Fn(&MailboxEnvelope) + Send + Sync>;

pub struct RelayRouter {
    online_peers: RwLock<HashMap<String, DeliverFn>>,
    max_envelope_size: usize,
}

impl RelayRouter {
    pub fn new(max_envelope_size: usize) -> Self {
        Self {
            online_peers: RwLock::new(HashMap::new()),
            max_envelope_size,
        }
    }

    pub fn route(&self, envelope: &MailboxEnvelope, mailbox: &BlindMailbox) -> RouteResult {
        if envelope.ciphertext.len() > self.max_envelope_size {
            return RouteResult::Rejected {
                reason: "envelope exceeds max size".into(),
            };
        }

        let peers = self.online_peers.read().unwrap();
        if let Some(deliver) = peers.get(&envelope.to) {
            deliver(envelope);
            RouteResult::Delivered {
                recipient_did: envelope.to.clone(),
            }
        } else {
            drop(peers);
            let _ = mailbox.deposit(envelope.clone());
            let size = mailbox.pending_count(&envelope.to);
            RouteResult::Queued {
                recipient_did: envelope.to.clone(),
                mailbox_size: size,
            }
        }
    }

    pub fn register_peer(&self, did: String, deliver: DeliverFn) {
        self.online_peers.write().unwrap().insert(did, deliver);
    }

    pub fn unregister_peer(&self, did: &str) {
        self.online_peers.write().unwrap().remove(did);
    }

    pub fn is_online(&self, did: &str) -> bool {
        self.online_peers.read().unwrap().contains_key(did)
    }

    pub fn online_peers(&self) -> Vec<String> {
        self.online_peers.read().unwrap().keys().cloned().collect()
    }
}

pub struct RelayRouterModule;

impl RelayModule for RelayRouterModule {
    fn name(&self) -> &str {
        "relay-router"
    }
    fn description(&self) -> &str {
        "Zero-knowledge envelope routing"
    }
    fn dependencies(&self) -> &[&str] {
        &["blind-mailbox"]
    }
    fn install(&self, ctx: &RelayContext) {
        ctx.set_capability(
            capabilities::ROUTER,
            RelayRouter::new(ctx.config.max_envelope_size_bytes),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

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
    fn route_to_online_peer() {
        let mb = BlindMailbox::new(60_000, 1024);
        let router = RelayRouter::new(1024);
        let delivered = Arc::new(AtomicBool::new(false));
        let d = Arc::clone(&delivered);
        router.register_peer(
            "alice".into(),
            Box::new(move |_| {
                d.store(true, Ordering::Relaxed);
            }),
        );

        let result = router.route(&make_envelope("alice"), &mb);
        assert!(matches!(result, RouteResult::Delivered { .. }));
        assert!(delivered.load(Ordering::Relaxed));
    }

    #[test]
    fn route_to_offline_queues() {
        let mb = BlindMailbox::new(60_000, 1024);
        let router = RelayRouter::new(1024);
        let result = router.route(&make_envelope("bob"), &mb);
        assert!(matches!(
            result,
            RouteResult::Queued {
                mailbox_size: 1,
                ..
            }
        ));
    }
}
