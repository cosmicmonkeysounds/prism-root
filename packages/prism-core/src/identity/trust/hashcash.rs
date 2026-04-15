//! Hashcash proof-of-work — spam protection for Relay messages.
//! Port of `createHashcashMinter` / `createHashcashVerifier` in
//! `trust/trust.ts`. Uses SHA-256 via the `sha2` crate (the TS source
//! used `crypto.subtle.digest`).

use chrono::{SecondsFormat, Utc};
use rand::RngCore;
use sha2::{Digest, Sha256};

use super::types::{HashcashChallenge, HashcashProof};

/// Stateless minter — brute-forces a counter until the SHA-256 hash of
/// the challenge string has the requested number of leading zero bits.
#[derive(Debug, Clone, Copy, Default)]
pub struct HashcashMinter;

impl HashcashMinter {
    pub fn mint(&self, challenge: &HashcashChallenge) -> HashcashProof {
        let mut counter: u64 = 0;
        loop {
            let input = hashcash_string(challenge, counter);
            let hash = sha256_hex(&input);
            if has_leading_zero_bits(&hash, challenge.bits) {
                return HashcashProof {
                    challenge: challenge.clone(),
                    counter,
                    hash,
                };
            }
            counter = counter.wrapping_add(1);
        }
    }
}

/// Verifier with a configurable default difficulty, used when calling
/// [`Self::create_challenge`] without an explicit bit count.
#[derive(Debug, Clone, Copy)]
pub struct HashcashVerifier {
    default_bits: u32,
}

impl Default for HashcashVerifier {
    fn default() -> Self {
        Self { default_bits: 8 }
    }
}

impl HashcashVerifier {
    pub fn with_default_bits(default_bits: u32) -> Self {
        Self { default_bits }
    }

    pub fn verify(&self, proof: &HashcashProof) -> bool {
        let input = hashcash_string(&proof.challenge, proof.counter);
        let hash = sha256_hex(&input);
        if hash != proof.hash {
            return false;
        }
        has_leading_zero_bits(&hash, proof.challenge.bits)
    }

    pub fn create_challenge(&self, resource: &str, bits: Option<u32>) -> HashcashChallenge {
        let mut random_bytes = [0u8; 8];
        rand::thread_rng().fill_bytes(&mut random_bytes);
        let salt = hex::encode(random_bytes);
        HashcashChallenge {
            resource: resource.to_string(),
            bits: bits.unwrap_or(self.default_bits),
            issued_at: Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true),
            salt,
        }
    }
}

pub fn create_hashcash_minter() -> HashcashMinter {
    HashcashMinter
}

pub fn create_hashcash_verifier(default_bits: u32) -> HashcashVerifier {
    HashcashVerifier::with_default_bits(default_bits)
}

fn hashcash_string(challenge: &HashcashChallenge, counter: u64) -> String {
    format!(
        "{}:{}:{}:{}:{}",
        challenge.resource, challenge.bits, challenge.issued_at, challenge.salt, counter
    )
}

fn sha256_hex(input: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    hex::encode(hasher.finalize())
}

fn has_leading_zero_bits(hex_hash: &str, bits: u32) -> bool {
    let full_nibbles = (bits / 4) as usize;
    let bytes = hex_hash.as_bytes();
    for &ch in bytes.iter().take(full_nibbles) {
        if ch != b'0' {
            return false;
        }
    }
    let remaining = bits % 4;
    if remaining > 0 {
        let Some(&ch) = bytes.get(full_nibbles) else {
            return false;
        };
        let nibble = (ch as char).to_digit(16).unwrap_or(0xf) as u8;
        let mask = 0xf_u8 << (4 - remaining);
        if nibble & mask != 0 {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_a_challenge() {
        let v = create_hashcash_verifier(8);
        let challenge = v.create_challenge("did:key:z123", Some(4));
        assert_eq!(challenge.resource, "did:key:z123");
        assert_eq!(challenge.bits, 4);
        assert!(!challenge.salt.is_empty());
        assert!(!challenge.issued_at.is_empty());
    }

    #[test]
    fn mints_a_valid_proof_for_low_difficulty() {
        let v = create_hashcash_verifier(8);
        let m = create_hashcash_minter();
        let challenge = v.create_challenge("relay-1", Some(4));
        let proof = m.mint(&challenge);
        assert!(!proof.hash.is_empty());
        assert_eq!(proof.hash.len(), 64);
        assert!(v.verify(&proof));
    }

    #[test]
    fn rejects_tampered_proof() {
        let v = create_hashcash_verifier(8);
        let m = create_hashcash_minter();
        let challenge = v.create_challenge("relay-1", Some(4));
        let proof = m.mint(&challenge);
        let tampered = HashcashProof {
            hash: "0".repeat(64),
            ..proof
        };
        assert!(!v.verify(&tampered));
    }

    #[test]
    fn rejects_proof_with_wrong_counter() {
        let v = create_hashcash_verifier(8);
        let m = create_hashcash_minter();
        let challenge = v.create_challenge("relay-1", Some(4));
        let proof = m.mint(&challenge);
        let wrong = HashcashProof {
            counter: proof.counter + 999_999,
            ..proof
        };
        assert!(!v.verify(&wrong));
    }

    #[test]
    fn uses_default_bits() {
        let v = create_hashcash_verifier(12);
        let challenge = v.create_challenge("test", None);
        assert_eq!(challenge.bits, 12);
    }
}
