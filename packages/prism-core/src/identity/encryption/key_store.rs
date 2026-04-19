//! `KeyStore` trait — pluggable secure storage for raw key material.
//!
//! Implementations:
//!
//! - [`MemoryKeyStore`] — in-process `HashMap`, useful for tests.
//! - A platform-keychain-backed impl will live in the shell/studio
//!   crate and implement this trait.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Secure storage for raw key material.
///
/// Unlike the legacy TS interface, this trait is synchronous. The
/// legacy API returned promises because it was targeting WebCrypto's
/// async `SubtleCrypto`; the Rust backends we have in scope (in-memory,
/// OS keychain via `keyring` / `security-framework`) are all
/// synchronous, and callers who want async can wrap this trait in a
/// `tokio::task::spawn_blocking`.
pub trait KeyStore: Send + Sync {
    /// Store raw key material under the given ID.
    fn set(&self, key_id: &str, material: &[u8]);
    /// Retrieve raw key material by ID. Returns `None` if not found.
    fn get(&self, key_id: &str) -> Option<Vec<u8>>;
    /// Delete key material by ID. Returns `true` iff the key existed.
    fn delete(&self, key_id: &str) -> bool;
    /// List all known key IDs.
    fn list(&self) -> Vec<String>;
}

/// In-memory key store for tests. Not suitable for production — key
/// material lives in process memory with no zeroization guarantees.
#[derive(Debug, Clone, Default)]
pub struct MemoryKeyStore {
    inner: Arc<Mutex<HashMap<String, Vec<u8>>>>,
}

impl MemoryKeyStore {
    pub fn new() -> Self {
        Self::default()
    }
}

impl KeyStore for MemoryKeyStore {
    fn set(&self, key_id: &str, material: &[u8]) {
        let mut guard = self.inner.lock().expect("MemoryKeyStore poisoned");
        guard.insert(key_id.to_string(), material.to_vec());
    }

    fn get(&self, key_id: &str) -> Option<Vec<u8>> {
        let guard = self.inner.lock().expect("MemoryKeyStore poisoned");
        guard.get(key_id).cloned()
    }

    fn delete(&self, key_id: &str) -> bool {
        let mut guard = self.inner.lock().expect("MemoryKeyStore poisoned");
        guard.remove(key_id).is_some()
    }

    fn list(&self) -> Vec<String> {
        let guard = self.inner.lock().expect("MemoryKeyStore poisoned");
        guard.keys().cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stores_and_retrieves_key_material() {
        let store = MemoryKeyStore::new();
        let material = [1u8, 2, 3, 4];
        store.set("key-1", &material);
        assert_eq!(store.get("key-1"), Some(material.to_vec()));
    }

    #[test]
    fn returns_none_for_missing_key() {
        let store = MemoryKeyStore::new();
        assert!(store.get("nonexistent").is_none());
    }

    #[test]
    fn deletes_key_material() {
        let store = MemoryKeyStore::new();
        store.set("key-1", &[1]);
        assert!(store.delete("key-1"));
        assert!(store.get("key-1").is_none());
    }

    #[test]
    fn lists_all_key_ids() {
        let store = MemoryKeyStore::new();
        store.set("a", &[1]);
        store.set("b", &[2]);
        let keys = store.list();
        assert!(keys.contains(&"a".to_string()));
        assert!(keys.contains(&"b".to_string()));
    }

    #[test]
    fn stores_a_defensive_copy() {
        let store = MemoryKeyStore::new();
        let mut original = vec![1u8, 2, 3];
        store.set("key-1", &original);

        original[0] = 99;
        let retrieved = store.get("key-1").unwrap();
        assert_eq!(retrieved[0], 1);
    }
}
