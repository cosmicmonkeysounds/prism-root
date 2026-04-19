//! CRDT document manager — the Loro-backed state pool the CRDT module
//! ships with by default.
//!
//! Lives in its own module so the `crdt` feature flag can cleanly gate it
//! without touching the rest of the crate.

use loro::LoroDoc;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::DaemonError;

/// Manages all active CRDT documents in the daemon.
pub struct DocManager {
    docs: Arc<Mutex<HashMap<String, LoroDoc>>>,
}

impl DocManager {
    pub fn new() -> Self {
        Self {
            docs: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Ensure a LoroDoc exists for the given ID, creating one if needed.
    pub fn get_or_create(&self, doc_id: &str) -> Result<(), DaemonError> {
        let mut docs = self.docs.lock().map_err(|_| DaemonError::LockPoisoned)?;
        if !docs.contains_key(doc_id) {
            docs.insert(doc_id.to_string(), LoroDoc::new());
        }
        Ok(())
    }

    /// Write a key-value pair to a document's root map.
    pub fn write(&self, doc_id: &str, key: &str, value: &str) -> Result<Vec<u8>, DaemonError> {
        let docs = self.docs.lock().map_err(|_| DaemonError::LockPoisoned)?;
        let doc = docs
            .get(doc_id)
            .ok_or_else(|| DaemonError::DocNotFound(doc_id.to_string()))?;
        let root = doc.get_map("root");
        root.insert(key, value)
            .map_err(|e| DaemonError::Loro(e.to_string()))?;
        doc.commit();
        let update = doc
            .export(loro::ExportMode::Snapshot)
            .map_err(|e| DaemonError::Loro(e.to_string()))?;
        Ok(update)
    }

    /// Read a value from a document's root map.
    pub fn read(&self, doc_id: &str, key: &str) -> Result<Option<String>, DaemonError> {
        let docs = self.docs.lock().map_err(|_| DaemonError::LockPoisoned)?;
        let doc = docs
            .get(doc_id)
            .ok_or_else(|| DaemonError::DocNotFound(doc_id.to_string()))?;
        let root = doc.get_map("root");
        let value = root.get(key).map(|v| {
            serde_json::to_string(&v.as_value().unwrap_or(&loro::LoroValue::Null))
                .unwrap_or_default()
        });
        Ok(value)
    }

    /// Export a document's full state.
    pub fn export_snapshot(&self, doc_id: &str) -> Result<Vec<u8>, DaemonError> {
        let docs = self.docs.lock().map_err(|_| DaemonError::LockPoisoned)?;
        let doc = docs
            .get(doc_id)
            .ok_or_else(|| DaemonError::DocNotFound(doc_id.to_string()))?;
        doc.export(loro::ExportMode::Snapshot)
            .map_err(|e| DaemonError::Loro(e.to_string()))
    }

    /// Import a snapshot into a document.
    pub fn import_snapshot(&self, doc_id: &str, data: &[u8]) -> Result<(), DaemonError> {
        let mut docs = self.docs.lock().map_err(|_| DaemonError::LockPoisoned)?;
        let doc = docs.entry(doc_id.to_string()).or_insert_with(LoroDoc::new);
        doc.import(data)
            .map_err(|e| DaemonError::Loro(e.to_string()))?;
        Ok(())
    }
}

impl Default for DocManager {
    fn default() -> Self {
        Self::new()
    }
}
