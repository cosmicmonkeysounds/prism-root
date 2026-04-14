//! `VaultKeyManager` — the stateful façade tying together HKDF,
//! AES-GCM-256, and a [`KeyStore`].
//!
//! Port of the `createVaultKeyManager` factory from
//! `identity/encryption/encryption.ts`. The TS version exposed methods
//! on a bag-of-closures object; here we use a struct with `&mut self`
//! methods because Rust's borrow checker makes that the idiomatic
//! shape (and it plays nicer with `Send + Sync` callers).

use std::collections::HashMap;
use std::sync::Arc;

use aes_gcm::aead::{Aead, KeyInit, Payload};
use aes_gcm::{Aes256Gcm, Nonce};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use chrono::Utc;
use hkdf::Hkdf;
use rand::rngs::OsRng;
use rand::RngCore;
use sha2::Sha256;

use super::error::EncryptionError;
use super::key_store::{KeyStore, MemoryKeyStore};
use super::types::{EncryptedSnapshot, VaultKeyInfo};

// ── Options ─────────────────────────────────────────────────────────────────

/// Options passed to [`VaultKeyManager::new`].
pub struct VaultKeyManagerOptions {
    /// Vault identifier used as salt context for key derivation.
    pub vault_id: String,
    /// Persistent key store. Defaults to an in-memory store if `None`.
    pub key_store: Option<Arc<dyn KeyStore>>,
}

impl VaultKeyManagerOptions {
    pub fn new(vault_id: impl Into<String>) -> Self {
        Self {
            vault_id: vault_id.into(),
            key_store: None,
        }
    }
}

// ── Key derivation ──────────────────────────────────────────────────────────

/// Run HKDF-SHA256 with the given IKM / salt / info and return 32 bytes
/// suitable for an AES-256 key.
fn derive_aes_key(ikm: &[u8], salt: &[u8], info: &[u8]) -> Result<[u8; 32], EncryptionError> {
    let hk = Hkdf::<Sha256>::new(Some(salt), ikm);
    let mut okm = [0u8; 32];
    hk.expand(info, &mut okm)
        .map_err(|e| EncryptionError::HkdfExpand(e.to_string()))?;
    Ok(okm)
}

fn generate_iv() -> [u8; 12] {
    let mut iv = [0u8; 12];
    OsRng.fill_bytes(&mut iv);
    iv
}

fn base64url_encode(bytes: &[u8]) -> String {
    URL_SAFE_NO_PAD.encode(bytes)
}

fn base64url_decode(input: &str) -> Result<Vec<u8>, EncryptionError> {
    URL_SAFE_NO_PAD
        .decode(input)
        .map_err(|e| EncryptionError::Base64Decode(e.to_string()))
}

// ── Encrypt / Decrypt ───────────────────────────────────────────────────────

fn aes_gcm_encrypt(
    key_bytes: &[u8; 32],
    plaintext: &[u8],
    aad: Option<&str>,
) -> Result<([u8; 12], Vec<u8>), EncryptionError> {
    let cipher = Aes256Gcm::new(key_bytes.into());
    let iv = generate_iv();
    let nonce = Nonce::from_slice(&iv);

    let ciphertext = match aad {
        Some(a) => cipher
            .encrypt(
                nonce,
                Payload {
                    msg: plaintext,
                    aad: a.as_bytes(),
                },
            )
            .map_err(|_| EncryptionError::EncryptFailed)?,
        None => cipher
            .encrypt(nonce, plaintext)
            .map_err(|_| EncryptionError::EncryptFailed)?,
    };

    Ok((iv, ciphertext))
}

fn aes_gcm_decrypt(
    key_bytes: &[u8; 32],
    iv: &[u8],
    ciphertext: &[u8],
    aad: Option<&str>,
) -> Result<Vec<u8>, EncryptionError> {
    let cipher = Aes256Gcm::new(key_bytes.into());
    let nonce = Nonce::from_slice(iv);

    match aad {
        Some(a) => cipher
            .decrypt(
                nonce,
                Payload {
                    msg: ciphertext,
                    aad: a.as_bytes(),
                },
            )
            .map_err(|_| EncryptionError::DecryptFailed),
        None => cipher
            .decrypt(nonce, ciphertext)
            .map_err(|_| EncryptionError::DecryptFailed),
    }
}

// ── Manager ─────────────────────────────────────────────────────────────────

/// Vault key manager — derives, rotates, and uses AES-GCM-256 keys
/// anchored to an identity's private Ed25519 bytes.
pub struct VaultKeyManager {
    vault_id: String,
    default_key_id: String,
    default_info: VaultKeyInfo,
    key_infos: HashMap<String, VaultKeyInfo>,
    crypto_keys: HashMap<String, [u8; 32]>,
    key_store: Arc<dyn KeyStore>,
}

impl VaultKeyManager {
    /// Build a new manager. Call [`Self::derive_vault_key`] before any
    /// encrypt / decrypt operations.
    pub fn new(options: VaultKeyManagerOptions) -> Self {
        let default_key_id = format!("vault-{}-default", options.vault_id);
        let default_info = VaultKeyInfo {
            key_id: default_key_id.clone(),
            collection_id: None,
            created: Utc::now().to_rfc3339(),
            rotated_at: None,
            version: 0,
        };
        let key_store = options
            .key_store
            .unwrap_or_else(|| Arc::new(MemoryKeyStore::new()) as Arc<dyn KeyStore>);

        Self {
            vault_id: options.vault_id,
            default_key_id,
            default_info,
            key_infos: HashMap::new(),
            crypto_keys: HashMap::new(),
            key_store,
        }
    }

    /// Get the default vault-wide key info (as created by
    /// [`Self::derive_vault_key`], or the stub returned before that
    /// has been called — mirrors legacy semantics).
    pub fn default_key_info(&self) -> &VaultKeyInfo {
        &self.default_info
    }

    // ── Derive vault key ────────────────────────────────────────────

    /// Derive and store a vault-wide key from the identity's private
    /// key material (typically the 32-byte Ed25519 seed, but any byte
    /// slice works — this is just the HKDF IKM).
    pub fn derive_vault_key(
        &mut self,
        private_key_bytes: &[u8],
    ) -> Result<VaultKeyInfo, EncryptionError> {
        let salt = format!("prism-vault-{}", self.vault_id);
        let info = b"prism-vault-key-v1";
        let raw_bytes = derive_aes_key(private_key_bytes, salt.as_bytes(), info)?;

        let new_info = VaultKeyInfo {
            key_id: self.default_key_id.clone(),
            collection_id: None,
            created: Utc::now().to_rfc3339(),
            rotated_at: None,
            version: 1,
        };

        self.default_info = new_info.clone();
        self.key_infos
            .insert(self.default_key_id.clone(), new_info.clone());
        self.crypto_keys
            .insert(self.default_key_id.clone(), raw_bytes);
        self.key_store.set(&self.default_key_id, &raw_bytes);

        Ok(new_info)
    }

    // ── Derive collection key ───────────────────────────────────────

    /// Derive a per-collection key from the vault key.
    pub fn derive_collection_key(
        &mut self,
        collection_id: &str,
    ) -> Result<VaultKeyInfo, EncryptionError> {
        let vault_raw = self
            .key_store
            .get(&self.default_key_id)
            .ok_or(EncryptionError::VaultKeyNotDerived)?;

        let salt = format!("prism-coll-{}", collection_id);
        let info = b"prism-collection-key-v1";
        let raw_bytes = derive_aes_key(&vault_raw, salt.as_bytes(), info)?;

        let key_id = format!("coll-{}-{}", self.vault_id, collection_id);
        let key_info = VaultKeyInfo {
            key_id: key_id.clone(),
            collection_id: Some(collection_id.to_string()),
            created: Utc::now().to_rfc3339(),
            rotated_at: None,
            version: 1,
        };

        self.key_infos.insert(key_id.clone(), key_info.clone());
        self.crypto_keys.insert(key_id.clone(), raw_bytes);
        self.key_store.set(&key_id, &raw_bytes);

        Ok(key_info)
    }

    // ── Rotate ──────────────────────────────────────────────────────

    /// Rotate an existing key (vault-wide or per-collection). Derives
    /// a new version from the existing material + a rotation-specific
    /// salt.
    pub fn rotate_key(&mut self, key_id: &str) -> Result<VaultKeyInfo, EncryptionError> {
        let existing_info = self
            .key_infos
            .get(key_id)
            .cloned()
            .ok_or_else(|| EncryptionError::KeyNotFound(key_id.to_string()))?;

        let existing_raw = self
            .key_store
            .get(key_id)
            .ok_or_else(|| EncryptionError::KeyNotFound(key_id.to_string()))?;

        let new_version = existing_info.version + 1;
        let salt = format!("prism-rotate-{}-v{}", key_id, new_version);
        let info = b"prism-key-rotation-v1";
        let raw_bytes = derive_aes_key(&existing_raw, salt.as_bytes(), info)?;

        let new_info = VaultKeyInfo {
            rotated_at: Some(Utc::now().to_rfc3339()),
            version: new_version,
            ..existing_info
        };

        if new_info.key_id == self.default_key_id {
            self.default_info = new_info.clone();
        }
        self.key_infos.insert(key_id.to_string(), new_info.clone());
        self.crypto_keys.insert(key_id.to_string(), raw_bytes);
        self.key_store.set(key_id, &raw_bytes);

        Ok(new_info)
    }

    // ── Encrypt / Decrypt ───────────────────────────────────────────

    fn get_crypto_key(&mut self, key_id: &str) -> Result<[u8; 32], EncryptionError> {
        if let Some(k) = self.crypto_keys.get(key_id) {
            return Ok(*k);
        }
        let raw = self
            .key_store
            .get(key_id)
            .ok_or_else(|| EncryptionError::KeyNotFound(key_id.to_string()))?;
        if raw.len() != 32 {
            return Err(EncryptionError::InvalidKeyLength {
                expected: 32,
                got: raw.len(),
            });
        }
        let mut key = [0u8; 32];
        key.copy_from_slice(&raw);
        self.crypto_keys.insert(key_id.to_string(), key);
        Ok(key)
    }

    /// Encrypt a Loro snapshot.
    pub fn encrypt_snapshot(
        &mut self,
        data: &[u8],
        key_id: Option<&str>,
        aad: Option<&str>,
    ) -> Result<EncryptedSnapshot, EncryptionError> {
        let resolved_key_id = key_id
            .map(|s| s.to_string())
            .unwrap_or_else(|| self.default_key_id.clone());
        let info = self
            .key_infos
            .get(&resolved_key_id)
            .cloned()
            .ok_or_else(|| EncryptionError::KeyNotFound(resolved_key_id.clone()))?;

        let crypto_key = self.get_crypto_key(&resolved_key_id)?;
        let (iv, ciphertext) = aes_gcm_encrypt(&crypto_key, data, aad)?;

        Ok(EncryptedSnapshot {
            key_id: resolved_key_id,
            key_version: info.version,
            iv: base64url_encode(&iv),
            ciphertext,
            aad: aad.map(|s| s.to_string()),
        })
    }

    /// Decrypt a Loro snapshot.
    pub fn decrypt_snapshot(
        &mut self,
        encrypted: &EncryptedSnapshot,
    ) -> Result<Vec<u8>, EncryptionError> {
        let crypto_key = self.get_crypto_key(&encrypted.key_id)?;
        let iv = base64url_decode(&encrypted.iv)?;
        aes_gcm_decrypt(
            &crypto_key,
            &iv,
            &encrypted.ciphertext,
            encrypted.aad.as_deref(),
        )
    }

    // ── Query ───────────────────────────────────────────────────────

    pub fn get_key_info(&self, key_id: &str) -> Option<&VaultKeyInfo> {
        self.key_infos.get(key_id)
    }

    pub fn list_keys(&self) -> Vec<VaultKeyInfo> {
        self.key_infos.values().cloned().collect()
    }

    // ── Dispose ─────────────────────────────────────────────────────

    /// Delete all key material from the store and clear in-memory caches.
    pub fn dispose(&mut self) {
        for key_id in self.key_infos.keys().cloned().collect::<Vec<_>>() {
            self.key_store.delete(&key_id);
        }
        self.key_infos.clear();
        self.crypto_keys.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::did::identity::{create_identity, CreateIdentityOptions};

    fn setup_manager() -> VaultKeyManager {
        let identity = create_identity(CreateIdentityOptions::default()).unwrap();
        let seed = identity.private_key_seed();
        let mut manager = VaultKeyManager::new(VaultKeyManagerOptions::new("test-vault"));
        manager.derive_vault_key(&seed).unwrap();
        manager
    }

    #[test]
    fn derives_a_vault_key() {
        let manager = setup_manager();
        let info = manager.default_key_info();
        assert!(info.key_id.contains("test-vault"));
        assert_eq!(info.version, 1);
        assert!(info.collection_id.is_none());
        assert!(info.rotated_at.is_none());
    }

    #[test]
    fn derives_per_collection_keys() {
        let mut manager = setup_manager();
        let coll_info = manager.derive_collection_key("notes").unwrap();
        assert!(coll_info.key_id.contains("notes"));
        assert_eq!(coll_info.collection_id.as_deref(), Some("notes"));
        assert_eq!(coll_info.version, 1);
    }

    #[test]
    fn derives_different_keys_for_different_collections() {
        let mut manager = setup_manager();
        manager.derive_collection_key("notes").unwrap();
        manager.derive_collection_key("tasks").unwrap();

        let keys = manager.list_keys();
        assert_eq!(keys.len(), 3); // default + notes + tasks

        let coll_ids: Vec<String> = keys
            .iter()
            .filter_map(|k| k.collection_id.clone())
            .collect();
        assert!(coll_ids.contains(&"notes".to_string()));
        assert!(coll_ids.contains(&"tasks".to_string()));
    }

    #[test]
    fn throws_when_deriving_collection_key_before_vault_key() {
        let mut manager = VaultKeyManager::new(VaultKeyManagerOptions::new("no-vault"));
        let err = manager.derive_collection_key("notes").unwrap_err();
        assert!(matches!(err, EncryptionError::VaultKeyNotDerived));
    }

    #[test]
    fn rotates_a_key() {
        let mut manager = setup_manager();
        let original_id = manager.default_key_info().key_id.clone();
        let rotated = manager.rotate_key(&original_id).unwrap();
        assert_eq!(rotated.version, 2);
        assert!(rotated.rotated_at.is_some());
    }

    #[test]
    fn throws_when_rotating_unknown_key() {
        let mut manager = setup_manager();
        let err = manager.rotate_key("nonexistent").unwrap_err();
        assert!(matches!(err, EncryptionError::KeyNotFound(_)));
    }

    #[test]
    fn encrypts_and_decrypts_a_snapshot() {
        let mut manager = setup_manager();
        let plaintext = b"loro-snapshot-data-here";

        let encrypted = manager.encrypt_snapshot(plaintext, None, None).unwrap();
        assert!(encrypted.key_id.contains("test-vault"));
        assert_eq!(encrypted.key_version, 1);
        assert!(!encrypted.iv.is_empty());
        assert!(encrypted.ciphertext.len() > plaintext.len()); // GCM adds 16-byte tag

        let decrypted = manager.decrypt_snapshot(&encrypted).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn encrypts_with_aad_and_requires_same_aad_for_decryption() {
        let mut manager = setup_manager();
        let plaintext = b"authenticated-data";

        let encrypted = manager
            .encrypt_snapshot(plaintext, None, Some("collection-123"))
            .unwrap();
        assert_eq!(encrypted.aad.as_deref(), Some("collection-123"));

        let decrypted = manager.decrypt_snapshot(&encrypted).unwrap();
        assert_eq!(decrypted, plaintext);

        // Tamper with AAD — should fail.
        let tampered = EncryptedSnapshot {
            aad: Some("collection-456".to_string()),
            ..encrypted
        };
        assert!(manager.decrypt_snapshot(&tampered).is_err());
    }

    #[test]
    fn encrypts_with_a_per_collection_key() {
        let mut manager = setup_manager();
        let coll_info = manager.derive_collection_key("secrets").unwrap();
        let plaintext = b"secret-snapshot";

        let encrypted = manager
            .encrypt_snapshot(plaintext, Some(&coll_info.key_id), None)
            .unwrap();
        assert_eq!(encrypted.key_id, coll_info.key_id);

        let decrypted = manager.decrypt_snapshot(&encrypted).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn produces_different_ciphertext_for_same_plaintext_random_iv() {
        let mut manager = setup_manager();
        let plaintext = b"same-data";

        let a = manager.encrypt_snapshot(plaintext, None, None).unwrap();
        let b = manager.encrypt_snapshot(plaintext, None, None).unwrap();

        assert_ne!(a.iv, b.iv);
        assert_ne!(a.ciphertext, b.ciphertext);
    }

    #[test]
    fn decrypts_after_key_rotation_using_rotated_key() {
        let mut manager = setup_manager();
        let plaintext = b"pre-rotation-data";

        let key_id = manager.default_key_info().key_id.clone();
        manager.rotate_key(&key_id).unwrap();

        let encrypted = manager.encrypt_snapshot(plaintext, None, None).unwrap();
        assert_eq!(encrypted.key_version, 2);

        let decrypted = manager.decrypt_snapshot(&encrypted).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn get_key_info_returns_info_for_existing_keys() {
        let manager = setup_manager();
        let key_id = manager.default_key_info().key_id.clone();
        let info = manager.get_key_info(&key_id);
        assert!(info.is_some());
        assert_eq!(info.unwrap().version, 1);
    }

    #[test]
    fn get_key_info_returns_none_for_unknown_keys() {
        let manager = setup_manager();
        assert!(manager.get_key_info("unknown").is_none());
    }

    #[test]
    fn dispose_clears_all_keys() {
        let mut manager = setup_manager();
        manager.derive_collection_key("notes").unwrap();
        assert_eq!(manager.list_keys().len(), 2);

        manager.dispose();
        assert!(manager.list_keys().is_empty());
    }
}
