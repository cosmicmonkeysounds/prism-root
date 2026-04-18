//! Hashcash — proof-of-work spam protection.
//!
//! Wraps `identity::trust::hashcash` primitives into a relay module.

use std::collections::HashSet;
use std::sync::RwLock;

use crate::identity::trust::hashcash::{create_hashcash_verifier, HashcashVerifier};
use crate::identity::trust::types::{HashcashChallenge, HashcashProof};
use crate::network::relay::module_system::{capabilities, RelayContext, RelayModule};

pub struct HashcashGate {
    verifier: HashcashVerifier,
    verified_dids: RwLock<HashSet<String>>,
}

impl HashcashGate {
    pub fn new(default_bits: u32) -> Self {
        Self {
            verifier: create_hashcash_verifier(default_bits),
            verified_dids: RwLock::new(HashSet::new()),
        }
    }

    pub fn create_challenge(&self, resource: &str) -> HashcashChallenge {
        self.verifier.create_challenge(resource, None)
    }

    pub fn verify_proof(&self, proof: &HashcashProof) -> bool {
        self.verifier.verify(proof)
    }

    pub fn is_verified(&self, did: &str) -> bool {
        self.verified_dids.read().unwrap().contains(did)
    }

    pub fn mark_verified(&self, did: &str) {
        self.verified_dids.write().unwrap().insert(did.to_string());
    }
}

pub struct HashcashModule {
    pub bits: u32,
}

impl HashcashModule {
    pub fn new(bits: u32) -> Self {
        Self { bits }
    }
}

impl Default for HashcashModule {
    fn default() -> Self {
        Self { bits: 16 }
    }
}

impl RelayModule for HashcashModule {
    fn name(&self) -> &str {
        "hashcash"
    }
    fn description(&self) -> &str {
        "Proof-of-work spam protection"
    }
    fn install(&self, ctx: &RelayContext) {
        ctx.set_capability(capabilities::HASHCASH, HashcashGate::new(self.bits));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verified_tracking() {
        let gate = HashcashGate::new(4);
        assert!(!gate.is_verified("alice"));
        gate.mark_verified("alice");
        assert!(gate.is_verified("alice"));
    }

    #[test]
    fn challenge_has_resource() {
        let gate = HashcashGate::new(4);
        let challenge = gate.create_challenge("test-resource");
        assert_eq!(challenge.resource, "test-resource");
    }
}
