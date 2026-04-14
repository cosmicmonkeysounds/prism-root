//! `identity::encryption` — vault-level encryption for Loro CRDT
//! snapshots at rest.
//!
//! Port of `packages/prism-core/src/identity/encryption/` from the
//! legacy TS tree. Keys are derived via HKDF-SHA256 from the identity's
//! Ed25519 private key, scoped per-vault and optionally per-collection.
//! Ciphertext is AES-GCM-256 with 12-byte random IVs and optional AAD.
//!
//! Submodules:
//!
//! - [`error`]              — [`EncryptionError`]
//! - [`types`]              — plain-data structs (`VaultKeyInfo`, `EncryptedSnapshot`)
//! - [`key_store`]          — [`KeyStore`] trait and in-memory impl
//! - [`vault_key_manager`]  — [`VaultKeyManager`] struct
//! - [`standalone`]         — one-off [`standalone::encrypt_snapshot`] / [`standalone::decrypt_snapshot`]

pub mod error;
pub mod key_store;
pub mod standalone;
pub mod types;
pub mod vault_key_manager;

pub use error::EncryptionError;
pub use key_store::{KeyStore, MemoryKeyStore};
pub use standalone::{decrypt_snapshot, encrypt_snapshot};
pub use types::{EncryptedSnapshot, VaultKeyInfo};
pub use vault_key_manager::{VaultKeyManager, VaultKeyManagerOptions};
