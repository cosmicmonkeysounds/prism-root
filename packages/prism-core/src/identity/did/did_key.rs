//! `did:key` multicodec encoding / decoding.
//!
//! Ed25519 public keys are prefixed with the multicodec bytes
//! `0xed 0x01` and then encoded as base58btc with the `z` multibase
//! prefix. See <https://w3c-ccg.github.io/did-method-key/>.

use super::base58::{decode_base58, encode_base58};
use super::error::IdentityError;

/// Ed25519 public key multicodec prefix: `0xed 0x01`.
const ED25519_MULTICODEC_PREFIX: [u8; 2] = [0xed, 0x01];

fn encode_multicodec_ed25519(public_key: &[u8; 32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(ED25519_MULTICODEC_PREFIX.len() + public_key.len());
    out.extend_from_slice(&ED25519_MULTICODEC_PREFIX);
    out.extend_from_slice(public_key);
    out
}

fn decode_multicodec_ed25519(bytes: &[u8]) -> Result<[u8; 32], IdentityError> {
    if bytes.len() < 2 || bytes[0] != 0xed || bytes[1] != 0x01 {
        return Err(IdentityError::NotEd25519Multicodec);
    }
    let rest = &bytes[2..];
    if rest.len() != 32 {
        return Err(IdentityError::InvalidKey(format!(
            "expected 32-byte Ed25519 public key, got {} bytes",
            rest.len()
        )));
    }
    let mut key = [0u8; 32];
    key.copy_from_slice(rest);
    Ok(key)
}

/// Multibase-encoded public key fragment (without the `did:key:` prefix).
///
/// Used inside the DID document's `publicKeyMultibase` field. Returns
/// `z` + base58btc(multicodec(publicKey)).
pub fn public_key_to_multibase(public_key: &[u8; 32]) -> String {
    let multicodec = encode_multicodec_ed25519(public_key);
    let mut out = String::with_capacity(1 + multicodec.len() * 2);
    out.push('z');
    out.push_str(&encode_base58(&multicodec));
    out
}

/// Encode an Ed25519 public key as a `did:key:z…` string.
pub fn public_key_to_did_key(public_key: &[u8; 32]) -> String {
    format!("did:key:{}", public_key_to_multibase(public_key))
}

/// Decode a `did:key:z…` string back to the 32-byte Ed25519 public key.
pub fn did_key_to_public_key(did: &str) -> Result<[u8; 32], IdentityError> {
    let parts: Vec<&str> = did.split(':').collect();
    if parts.len() < 3 || parts[0] != "did" || parts[1] != "key" {
        return Err(IdentityError::InvalidDidKey(did.to_string()));
    }
    let multibase = parts[2..].join(":");
    if !multibase.starts_with('z') {
        return Err(IdentityError::UnsupportedMultibase(multibase));
    }
    let decoded = decode_base58(&multibase[1..])?;
    decode_multicodec_ed25519(&decoded)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::RngCore;

    #[test]
    fn round_trips_a_public_key_through_did_key() {
        let mut pub_key = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut pub_key);

        let did = public_key_to_did_key(&pub_key);
        assert!(did.starts_with("did:key:z"));

        let recovered = did_key_to_public_key(&did).unwrap();
        assert_eq!(recovered, pub_key);
    }

    #[test]
    fn throws_on_invalid_did_key_format() {
        let err = did_key_to_public_key("not-a-did").unwrap_err();
        assert!(matches!(err, IdentityError::InvalidDidKey(_)));
    }

    #[test]
    fn throws_on_non_z_multibase_prefix() {
        let err = did_key_to_public_key("did:key:m123").unwrap_err();
        assert!(matches!(err, IdentityError::UnsupportedMultibase(_)));
    }
}
