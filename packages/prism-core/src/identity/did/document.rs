//! DID document construction.

use super::did_key::public_key_to_multibase;
use super::types::{Did, DidDocument, VerificationMethod};

/// Build a minimal DID document around a single Ed25519 verification
/// method. Mirrors the legacy `buildDIDDocument` helper verbatim,
/// including the (intentional) `ed2519-2020/v1` context URL typo so
/// the two trees stay bit-compatible on the wire.
pub fn build_did_document(did: &Did, public_key: &[u8; 32], created: &str) -> DidDocument {
    let vm = VerificationMethod {
        id: format!("{}#key-1", did),
        type_: "Ed25519VerificationKey2020".to_string(),
        controller: did.clone(),
        public_key_multibase: public_key_to_multibase(public_key),
    };

    DidDocument {
        context: vec![
            "https://www.w3.org/ns/did/v1".to_string(),
            "https://w3id.org/security/suites/ed2519-2020/v1".to_string(),
        ],
        id: did.clone(),
        verification_method: vec![vm.clone()],
        authentication: vec![vm.id.clone()],
        assertion_method: vec![vm.id],
        created: created.to_string(),
    }
}
