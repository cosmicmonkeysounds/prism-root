//! Plain-data structs used throughout the encryption module.
//!
//! Serde field names stay camelCase so JSON written by the legacy TS
//! `encryptSnapshot` call is byte-compatible with Rust's output.

use serde::{Deserialize, Serialize};

/// Metadata for a vault encryption key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VaultKeyInfo {
    /// Unique key identifier (e.g. `"vault-key-1"`).
    pub key_id: String,
    /// Which collection this key encrypts (`None` = vault-wide default).
    pub collection_id: Option<String>,
    /// ISO-8601 creation timestamp.
    pub created: String,
    /// ISO-8601 rotation timestamp (`None` = never rotated).
    pub rotated_at: Option<String>,
    /// Key version for rotation tracking.
    pub version: u32,
}

/// An encrypted snapshot blob with metadata for decryption.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EncryptedSnapshot {
    /// Key ID used for encryption.
    pub key_id: String,
    /// Key version at the time of encryption.
    pub key_version: u32,
    /// 12-byte IV as base64url.
    pub iv: String,
    /// Encrypted ciphertext bytes (includes the 16-byte AES-GCM tag).
    #[serde(with = "serde_bytes")]
    pub ciphertext: Vec<u8>,
    /// Optional associated-data tag (e.g. collection ID).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aad: Option<String>,
}

// Tiny serde_bytes stand-in so we don't have to add another workspace
// dep just for one field. Encodes `Vec<u8>` as a byte-aware sequence
// (serde_json emits it as a plain number array, matching JS Uint8Array
// JSON semantics out of the box; binary formats like bincode use the
// `Bytes` type directly).
mod serde_bytes {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<S>(bytes: &Vec<u8>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        bytes.serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Vec<u8>, D::Error>
    where
        D: Deserializer<'de>,
    {
        Vec::<u8>::deserialize(deserializer)
    }
}
