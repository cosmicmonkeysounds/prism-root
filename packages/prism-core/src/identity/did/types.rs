//! Plain-data types for DIDs and DID documents.
//!
//! Mirrors `identity-types.ts`. Serde field names stay camelCase so
//! that exported/imported JSON stays interchangeable with anything
//! the legacy package wrote to disk.

use serde::{Deserialize, Serialize};

/// W3C DID string (`did:method:id`).
pub type Did = String;

/// DID method — currently `key` and `web` are supported.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DidMethod {
    #[default]
    Key,
    Web,
}

/// Verification method inside a DID document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VerificationMethod {
    pub id: String,
    /// Always `"Ed25519VerificationKey2020"` today.
    #[serde(rename = "type")]
    pub type_: String,
    pub controller: String,
    /// Base58btc-encoded public key with multibase `z` prefix.
    pub public_key_multibase: String,
}

/// Simplified W3C DID document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DidDocument {
    #[serde(rename = "@context")]
    pub context: Vec<String>,
    pub id: Did,
    pub verification_method: Vec<VerificationMethod>,
    pub authentication: Vec<String>,
    pub assertion_method: Vec<String>,
    pub created: String,
}
