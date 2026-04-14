//! `EncryptionError` — fallible operations across the encryption module.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum EncryptionError {
    #[error("key not found: {0}")]
    KeyNotFound(String),

    #[error("vault key not yet derived — call derive_vault_key() first")]
    VaultKeyNotDerived,

    #[error("HKDF expand failed: {0}")]
    HkdfExpand(String),

    #[error("AES-GCM encrypt failed")]
    EncryptFailed,

    #[error("AES-GCM decrypt failed")]
    DecryptFailed,

    #[error("invalid key length: expected {expected}, got {got}")]
    InvalidKeyLength { expected: usize, got: usize },

    #[error("base64 decode error: {0}")]
    Base64Decode(String),
}
