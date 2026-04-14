//! [`PrismIdentity`] — the runtime identity struct plus its
//! create / resolve / import / export surface.
//!
//! Legacy source: `identity/did/identity.ts` functions `createIdentity`,
//! `resolveIdentity`, `signPayload`, `verifySignature`, `exportIdentity`,
//! `importIdentity`.
//!
//! The legacy code kept sign/verify as `CryptoKey` handles bound to
//! `SubtleCrypto`. Here they're backed by ed25519-dalek's `SigningKey`
//! / `VerifyingKey`. The exported JSON shape mirrors the TS tree so
//! identities persisted by the legacy `exportIdentity` helper can still
//! be loaded, and vice versa — with one caveat: the TS version emitted
//! JWK-wrapped bytes, the Rust version emits raw base64url seeds for
//! simplicity. Round-tripping between the two languages requires a
//! small shim on the TS side (extract `d` / `x` from the JWK).

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use chrono::Utc;
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};

use super::did_key::{did_key_to_public_key, public_key_to_did_key};
use super::document::build_did_document;
use super::error::IdentityError;
use super::types::{Did, DidDocument, DidMethod};

// ── Options ─────────────────────────────────────────────────────────────────

/// Options controlling [`create_identity`].
#[derive(Debug, Clone, Default)]
pub struct CreateIdentityOptions {
    /// DID method to use. Default `DidMethod::Key`.
    pub method: DidMethod,
    /// For `did:web` — the domain (e.g. `"example.com"`). Required when
    /// `method == DidMethod::Web`.
    pub domain: Option<String>,
    /// Optional path for `did:web` (e.g. `"users/alice"`).
    pub path: Option<String>,
}

// ── PrismIdentity ───────────────────────────────────────────────────────────

/// Fully-resolved Prism identity with signing capability.
#[derive(Debug, Clone)]
pub struct PrismIdentity {
    pub did: Did,
    pub document: DidDocument,
    signing_key: SigningKey,
}

impl PrismIdentity {
    /// Raw 32-byte public key bytes.
    pub fn public_key_bytes(&self) -> [u8; 32] {
        self.signing_key.verifying_key().to_bytes()
    }

    /// Raw 32-byte seed (the Ed25519 private key material).
    pub fn private_key_seed(&self) -> [u8; 32] {
        self.signing_key.to_bytes()
    }

    /// Borrow the underlying `SigningKey`.
    pub fn signing_key(&self) -> &SigningKey {
        &self.signing_key
    }

    /// Sign arbitrary payload bytes. Returns a 64-byte Ed25519 signature.
    pub fn sign_payload(&self, data: &[u8]) -> [u8; 64] {
        self.signing_key.sign(data).to_bytes()
    }

    /// Verify a signature against this identity's public key.
    pub fn verify_signature(&self, data: &[u8], signature: &[u8]) -> bool {
        let Ok(sig) = Signature::from_slice(signature) else {
            return false;
        };
        self.signing_key.verifying_key().verify(data, &sig).is_ok()
    }
}

/// Resolved identity — public key only, no signing capability.
#[derive(Debug, Clone)]
pub struct ResolvedIdentity {
    pub did: Did,
    pub document: DidDocument,
    pub public_key: [u8; 32],
}

impl ResolvedIdentity {
    /// Verify a signature against this identity's public key.
    pub fn verify_signature(&self, data: &[u8], signature: &[u8]) -> bool {
        let Ok(verifying) = VerifyingKey::from_bytes(&self.public_key) else {
            return false;
        };
        let Ok(sig) = Signature::from_slice(signature) else {
            return false;
        };
        verifying.verify(data, &sig).is_ok()
    }
}

// ── Exported / imported JSON persistence ────────────────────────────────────

/// Serialized identity for file-based persistence. JSON-safe.
///
/// Note: the legacy TS version stored keys as JWKs. The Rust port
/// stores base64url-encoded raw seeds under the same JSON key names
/// for easier interchange with the rest of the ecosystem. If the
/// migration plan requires JWK compat, wire up a small conversion in
/// the persistence layer that calls this module — the fields that
/// need to round-trip (`did`, `createdAt`) keep the same name.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportedIdentity {
    pub did: Did,
    /// Base64url-encoded 32-byte Ed25519 seed.
    pub private_key_seed: String,
    /// Base64url-encoded 32-byte Ed25519 public key.
    pub public_key: String,
    /// ISO-8601 timestamp of when the identity was created.
    pub created_at: String,
}

// ── Helpers ─────────────────────────────────────────────────────────────────

/// Base64url-encode bytes without padding. Exported for symmetry with
/// the legacy tree so callers that serialised other buffers can reach
/// the same helper here.
pub fn base64url_encode(bytes: &[u8]) -> String {
    URL_SAFE_NO_PAD.encode(bytes)
}

fn base64url_decode(input: &str) -> Result<Vec<u8>, IdentityError> {
    URL_SAFE_NO_PAD
        .decode(input)
        .map_err(|e| IdentityError::Base64Decode(e.to_string()))
}

fn build_did_web(domain: &str, path: Option<&str>) -> Did {
    let encoded_domain = domain.replace(':', "%3A");
    match path {
        Some(p) => {
            let encoded_path = p
                .split('/')
                .map(urlencode_segment)
                .collect::<Vec<_>>()
                .join(":");
            format!("did:web:{}:{}", encoded_domain, encoded_path)
        }
        None => format!("did:web:{}", encoded_domain),
    }
}

// Small URL-encoding helper that matches JS `encodeURIComponent` semantics
// for the characters the legacy did:web path builder actually produces
// (A-Za-z0-9 plus `-_.!~*'()`). We don't pull in `url`/`urlencoding` just
// for this narrow use-case.
fn urlencode_segment(segment: &str) -> String {
    let mut out = String::with_capacity(segment.len());
    for ch in segment.chars() {
        if ch.is_ascii_alphanumeric()
            || matches!(ch, '-' | '_' | '.' | '!' | '~' | '*' | '\'' | '(' | ')')
        {
            out.push(ch);
        } else {
            let mut buf = [0u8; 4];
            for byte in ch.encode_utf8(&mut buf).as_bytes() {
                out.push_str(&format!("%{:02X}", byte));
            }
        }
    }
    out
}

// ── Public API ──────────────────────────────────────────────────────────────

/// Generate a new Ed25519 identity with a DID document.
pub fn create_identity(options: CreateIdentityOptions) -> Result<PrismIdentity, IdentityError> {
    let signing_key = SigningKey::generate(&mut OsRng);
    let public_key_bytes = signing_key.verifying_key().to_bytes();

    let did = match options.method {
        DidMethod::Key => public_key_to_did_key(&public_key_bytes),
        DidMethod::Web => {
            let domain = options
                .domain
                .as_deref()
                .ok_or(IdentityError::DidWebMissingDomain)?;
            build_did_web(domain, options.path.as_deref())
        }
    };

    let document = build_did_document(&did, &public_key_bytes, &Utc::now().to_rfc3339());
    Ok(PrismIdentity {
        did,
        document,
        signing_key,
    })
}

/// Resolve a DID to its public key and verification method.
///
/// For `did:key`, the public key is extracted directly from the DID
/// string. For `did:web`, a DID-document resolver is needed (not yet
/// implemented — matches legacy behavior).
pub fn resolve_identity(did: &str) -> Result<ResolvedIdentity, IdentityError> {
    let parts: Vec<&str> = did.split(':').collect();
    let method = parts.get(1).copied().unwrap_or("");

    match method {
        "key" => {
            let public_key = did_key_to_public_key(did)?;
            let document =
                build_did_document(&did.to_string(), &public_key, &Utc::now().to_rfc3339());
            Ok(ResolvedIdentity {
                did: did.to_string(),
                document,
                public_key,
            })
        }
        "web" => Err(IdentityError::DidWebNotImplemented),
        other => Err(IdentityError::UnsupportedDidMethod(other.to_string())),
    }
}

/// Convenience wrapper: sign a payload with a [`PrismIdentity`].
pub fn sign_payload(identity: &PrismIdentity, data: &[u8]) -> [u8; 64] {
    identity.sign_payload(data)
}

/// Verify a signature against a DID. Resolves the DID to get the public key.
pub fn verify_signature(did: &str, data: &[u8], signature: &[u8]) -> Result<bool, IdentityError> {
    let resolved = resolve_identity(did)?;
    Ok(resolved.verify_signature(data, signature))
}

/// Export a [`PrismIdentity`] to a JSON-safe struct for file persistence.
pub fn export_identity(identity: &PrismIdentity) -> ExportedIdentity {
    ExportedIdentity {
        did: identity.did.clone(),
        private_key_seed: base64url_encode(&identity.private_key_seed()),
        public_key: base64url_encode(&identity.public_key_bytes()),
        created_at: identity.document.created.clone(),
    }
}

/// Import a [`PrismIdentity`] from a previously-exported struct.
pub fn import_identity(exported: &ExportedIdentity) -> Result<PrismIdentity, IdentityError> {
    let seed_bytes = base64url_decode(&exported.private_key_seed)?;
    if seed_bytes.len() != 32 {
        return Err(IdentityError::InvalidKey(format!(
            "expected 32-byte seed, got {}",
            seed_bytes.len()
        )));
    }
    let mut seed = [0u8; 32];
    seed.copy_from_slice(&seed_bytes);
    let signing_key = SigningKey::from_bytes(&seed);
    let public_key_bytes = signing_key.verifying_key().to_bytes();
    let document = build_did_document(&exported.did, &public_key_bytes, &exported.created_at);
    Ok(PrismIdentity {
        did: exported.did.clone(),
        document,
        signing_key,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── createIdentity ──────────────────────────────────────────────────────

    #[test]
    fn creates_a_did_key_identity_by_default() {
        let identity = create_identity(CreateIdentityOptions::default()).unwrap();
        assert!(identity.did.starts_with("did:key:z"));
        assert_eq!(identity.document.id, identity.did);
        assert!(identity
            .document
            .context
            .contains(&"https://www.w3.org/ns/did/v1".to_string()));
        assert_eq!(identity.document.verification_method.len(), 1);
        assert_eq!(identity.document.authentication.len(), 1);
        assert_eq!(identity.document.assertion_method.len(), 1);
        assert_eq!(identity.public_key_bytes().len(), 32);
    }

    #[test]
    fn creates_a_did_web_identity() {
        let identity = create_identity(CreateIdentityOptions {
            method: DidMethod::Web,
            domain: Some("example.com".to_string()),
            path: None,
        })
        .unwrap();
        assert_eq!(identity.did, "did:web:example.com");
        assert_eq!(identity.document.id, "did:web:example.com");
    }

    #[test]
    fn creates_a_did_web_identity_with_path() {
        let identity = create_identity(CreateIdentityOptions {
            method: DidMethod::Web,
            domain: Some("example.com".to_string()),
            path: Some("users/alice".to_string()),
        })
        .unwrap();
        assert_eq!(identity.did, "did:web:example.com:users:alice");
    }

    #[test]
    fn throws_when_did_web_is_missing_domain() {
        let err = create_identity(CreateIdentityOptions {
            method: DidMethod::Web,
            domain: None,
            path: None,
        })
        .unwrap_err();
        assert!(matches!(err, IdentityError::DidWebMissingDomain));
    }

    #[test]
    fn generates_unique_identities_each_time() {
        let a = create_identity(CreateIdentityOptions::default()).unwrap();
        let b = create_identity(CreateIdentityOptions::default()).unwrap();
        assert_ne!(a.did, b.did);
    }

    // ── sign / verify ───────────────────────────────────────────────────────

    #[test]
    fn signs_and_verifies_a_payload() {
        let identity = create_identity(CreateIdentityOptions::default()).unwrap();
        let data = b"hello prism";

        let signature = sign_payload(&identity, data);
        assert_eq!(signature.len(), 64);

        assert!(identity.verify_signature(data, &signature));
    }

    #[test]
    fn rejects_tampered_data() {
        let identity = create_identity(CreateIdentityOptions::default()).unwrap();
        let signature = identity.sign_payload(b"original");
        assert!(!identity.verify_signature(b"tampered", &signature));
    }

    #[test]
    fn rejects_tampered_signature() {
        let identity = create_identity(CreateIdentityOptions::default()).unwrap();
        let mut signature = identity.sign_payload(b"hello");
        signature[0] ^= 0xff;
        assert!(!identity.verify_signature(b"hello", &signature));
    }

    #[test]
    fn verifies_via_standalone_verify_signature_with_did_resolution() {
        let identity = create_identity(CreateIdentityOptions::default()).unwrap();
        let data = b"verify-me";
        let sig = identity.sign_payload(data);

        let valid = verify_signature(&identity.did, data, &sig).unwrap();
        assert!(valid);
    }

    // ── resolveIdentity ─────────────────────────────────────────────────────

    #[test]
    fn resolves_a_did_key_to_its_public_key() {
        let identity = create_identity(CreateIdentityOptions::default()).unwrap();
        let resolved = resolve_identity(&identity.did).unwrap();

        assert_eq!(resolved.did, identity.did);
        assert_eq!(resolved.public_key, identity.public_key_bytes());
        assert_eq!(resolved.document.verification_method.len(), 1);
    }

    #[test]
    fn resolved_identity_can_verify_signatures_from_original() {
        let identity = create_identity(CreateIdentityOptions::default()).unwrap();
        let data = b"cross-verify";
        let sig = identity.sign_payload(data);

        let resolved = resolve_identity(&identity.did).unwrap();
        assert!(resolved.verify_signature(data, &sig));
    }

    #[test]
    fn throws_for_did_web_not_yet_implemented() {
        let err = resolve_identity("did:web:example.com").unwrap_err();
        assert!(matches!(err, IdentityError::DidWebNotImplemented));
    }

    #[test]
    fn throws_for_unsupported_did_method() {
        let err = resolve_identity("did:btcr:abc123").unwrap_err();
        assert!(matches!(err, IdentityError::UnsupportedDidMethod(_)));
    }

    // ── identity persistence ────────────────────────────────────────────────

    #[test]
    fn round_trips_an_identity_through_export_import() {
        let original = create_identity(CreateIdentityOptions::default()).unwrap();
        let exported = export_identity(&original);

        assert_eq!(exported.did, original.did);
        assert!(!exported.private_key_seed.is_empty());
        assert!(!exported.public_key.is_empty());
        assert!(!exported.created_at.is_empty());

        let restored = import_identity(&exported).unwrap();
        assert_eq!(restored.did, original.did);
        assert_eq!(restored.public_key_bytes(), original.public_key_bytes());
    }

    #[test]
    fn restored_identity_can_sign_and_original_can_verify() {
        let original = create_identity(CreateIdentityOptions::default()).unwrap();
        let exported = export_identity(&original);
        let restored = import_identity(&exported).unwrap();

        let data = b"persistence-test";
        let sig = restored.sign_payload(data);
        assert!(original.verify_signature(data, &sig));
    }

    #[test]
    fn original_identity_can_sign_and_restored_can_verify() {
        let original = create_identity(CreateIdentityOptions::default()).unwrap();
        let exported = export_identity(&original);
        let restored = import_identity(&exported).unwrap();

        let data = b"reverse-test";
        let sig = original.sign_payload(data);
        assert!(restored.verify_signature(data, &sig));
    }

    #[test]
    fn exported_identity_is_json_serializable() {
        let identity = create_identity(CreateIdentityOptions::default()).unwrap();
        let exported = export_identity(&identity);

        let json = serde_json::to_string(&exported).unwrap();
        let parsed: ExportedIdentity = serde_json::from_str(&json).unwrap();
        let restored = import_identity(&parsed).unwrap();

        assert_eq!(restored.did, identity.did);

        let data = b"json-round-trip";
        let sig = restored.sign_payload(data);
        assert!(identity.verify_signature(data, &sig));
    }
}
