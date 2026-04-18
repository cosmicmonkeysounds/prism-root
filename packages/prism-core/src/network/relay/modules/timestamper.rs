//! Relay timestamper — cryptographic proof-of-when receipts.

use serde::{Deserialize, Serialize};

use crate::network::relay::module_system::{capabilities, RelayContext, RelayModule};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TimestampReceipt {
    pub data_hash: String,
    pub timestamp: String,
    pub relay_did: String,
    pub signature: Vec<u8>,
}

pub struct RelayTimestamper {
    relay_did: String,
}

impl RelayTimestamper {
    pub fn new(relay_did: String) -> Self {
        Self { relay_did }
    }

    pub fn stamp(&self, data_hash: &str, now_iso: &str) -> TimestampReceipt {
        let message = format!("{data_hash}:{now_iso}");
        // In production, this signs with the relay's Ed25519 key.
        // For now, use a placeholder signature (the signing key is
        // injected by the host crate that owns the identity).
        let signature = sha2_hash(message.as_bytes());
        TimestampReceipt {
            data_hash: data_hash.to_string(),
            timestamp: now_iso.to_string(),
            relay_did: self.relay_did.clone(),
            signature,
        }
    }

    pub fn verify(&self, receipt: &TimestampReceipt) -> bool {
        let message = format!("{}:{}", receipt.data_hash, receipt.timestamp);
        let expected = sha2_hash(message.as_bytes());
        receipt.signature == expected && receipt.relay_did == self.relay_did
    }
}

fn sha2_hash(data: &[u8]) -> Vec<u8> {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(data);
    hasher.finalize().to_vec()
}

pub struct RelayTimestampModule;

impl RelayModule for RelayTimestampModule {
    fn name(&self) -> &str {
        "relay-timestamp"
    }
    fn description(&self) -> &str {
        "Cryptographic proof-of-when receipts"
    }
    fn install(&self, ctx: &RelayContext) {
        ctx.set_capability(
            capabilities::TIMESTAMPER,
            RelayTimestamper::new(ctx.config.relay_did.clone()),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stamp_and_verify() {
        let ts = RelayTimestamper::new("did:key:relay".into());
        let receipt = ts.stamp("abc123", "2026-04-18T00:00:00Z");
        assert!(ts.verify(&receipt));
    }

    #[test]
    fn tampered_receipt_fails() {
        let ts = RelayTimestamper::new("did:key:relay".into());
        let mut receipt = ts.stamp("abc123", "2026-04-18T00:00:00Z");
        receipt.data_hash = "tampered".into();
        assert!(!ts.verify(&receipt));
    }
}
